// SPDX-License-Identifier: MIT
use crate::{database, discovery, display, runtime};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
};

pub struct Launcher {
    pub data: PathBuf,
    db: PathBuf,
    logs: PathBuf,
    launch_lock: Mutex<()>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Game {
    pub id: i64,
    pub name: String,
    pub executable: String,
    pub source: String,
    pub runtime_kind: String,
    pub runtime_path: String,
    pub arguments: Vec<String>,
    pub prefix: String,
    pub available: bool,
    pub last_played_at: Option<String>,
    pub launch: Option<Launch>,
    pub tools: Vec<Tool>,
    /// Total seconds across play sessions whose end time is known.
    pub play_seconds: i64,
    /// Most recent play sessions, newest first.
    pub sessions: Vec<Session>,
    /// Known play time per UTC day over the last 14 days, oldest first; days without play omitted.
    pub daily_play: Vec<DayPlay>,
    pub display_mode: String,
    pub display_width: Option<u32>,
    pub display_height: Option<u32>,
    /// Whether Respawn+ can set this game's window mode (see `display`).
    pub display_supported: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DayPlay {
    pub date: String,
    pub seconds: i64,
}

/// A game launch, direct or through a script extender. Companion-only launches are not play.
#[derive(Clone, Debug, Serialize)]
pub struct Session {
    pub started_at: String,
    pub ended_at: Option<String>,
    pub state: String,
    /// Known duration; `None` while unknown (the session ended while Respawn+ was not watching).
    pub seconds: Option<i64>,
    /// Companion profile that started the game, when it was not launched directly.
    pub via: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Tool {
    pub id: i64,
    pub game_id: i64,
    pub kind: String,
    pub name: String,
    pub executable: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
    pub available: bool,
    pub launch: Option<Launch>,
}

#[derive(Deserialize)]
pub struct ToolSettings {
    pub id: Option<i64>,
    pub game_id: i64,
    #[serde(default = "default_tool_kind")]
    pub kind: String,
    pub name: String,
    pub executable: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
}

fn default_tool_kind() -> String {
    "mod_manager".into()
}

#[derive(Clone, Debug, Serialize)]
pub struct Launch {
    pub id: i64,
    pub state: String,
    pub exit_code: Option<i32>,
    pub log_path: String,
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct GameSettings {
    pub id: i64,
    pub name: String,
    pub runtime_kind: String,
    pub runtime_path: String,
    pub arguments: Vec<String>,
    #[serde(default = "default_display_mode")]
    pub display_mode: String,
    #[serde(default)]
    pub display_width: Option<u32>,
    #[serde(default)]
    pub display_height: Option<u32>,
}

fn default_display_mode() -> String {
    "default".into()
}

#[derive(Serialize)]
pub struct ScanReport {
    pub added: usize,
    pub discovered: usize,
    pub roots: Vec<String>,
    pub limited: bool,
    pub skipped_directories: usize,
}

impl Launcher {
    pub fn new() -> Result<Self, String> {
        let data = dirs::data_dir()
            .ok_or("Cannot locate app data")?
            .join("respawn-plus");
        let logs = dirs::state_dir()
            .unwrap_or_else(|| data.clone())
            .join("respawn-plus/logs/library");
        Self::at(data, logs)
    }

    pub fn at(data: PathBuf, logs: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&data).map_err(|e| e.to_string())?;
        let db = data.join("respawn-plus.sqlite3");
        database::open(&db).map_err(|e| e.to_string())?;
        Ok(Self {
            data,
            db,
            logs,
            launch_lock: Mutex::new(()),
        })
    }

    fn connection(&self) -> Result<Connection, String> {
        database::open(&self.db).map_err(|e| e.to_string())
    }

    pub fn games(&self) -> Result<Vec<Game>, String> {
        let db = self.connection()?;
        reconcile(&db)?;
        let mut query = db.prepare("SELECT g.id,g.name,g.executable,g.source,g.runtime_kind,g.runtime_path,g.arguments_json,g.last_played_at,p.path,l.id,l.state,l.exit_code,l.log_path,l.error,g.display_mode,g.display_width,g.display_height FROM games g LEFT JOIN prefixes p ON p.id=(SELECT id FROM prefixes WHERE game_id=g.id ORDER BY id LIMIT 1) LEFT JOIN launches l ON l.id=(SELECT MAX(id) FROM launches WHERE game_id=g.id AND tool_id IS NULL) ORDER BY g.name COLLATE NOCASE").map_err(|e| e.to_string())?;
        let rows = query
            .query_map([], |row| {
                let executable: String = row.get::<_, Option<String>>(2)?.unwrap_or_default();
                let args: String = row.get(6)?;
                let launch_id: Option<i64> = row.get(9)?;
                let (play_seconds, sessions, daily_play) = sessions_for_game(&db, row.get(0)?)?;
                Ok(Game {
                    play_seconds,
                    sessions,
                    daily_play,
                    id: row.get(0)?,
                    name: row.get(1)?,
                    available: Path::new(&executable).is_file(),
                    display_supported: display::supported(Path::new(&executable)),
                    executable,
                    source: row.get(3)?,
                    runtime_kind: row.get(4)?,
                    runtime_path: row.get(5)?,
                    arguments: serde_json::from_str(&args).unwrap_or_default(),
                    last_played_at: row.get(7)?,
                    display_mode: row.get(14)?,
                    display_width: row.get(15)?,
                    display_height: row.get(16)?,
                    prefix: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
                    tools: tools_for_game(&db, row.get(0)?)?,
                    launch: match launch_id {
                        Some(id) => Some(Launch {
                            id,
                            state: row.get(10)?,
                            exit_code: row.get(11)?,
                            log_path: row.get(12)?,
                            error: row.get(13)?,
                        }),
                        None => None,
                    },
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn import(
        &self,
        executable: &Path,
        name: Option<&str>,
        source: &str,
    ) -> Result<Game, String> {
        let executable = validate_executable(executable)?;
        let install = executable.parent().ok_or("Game has no parent directory")?;
        let default = runtime::discover()
            .into_iter()
            .find(|r| r.kind == "proton")
            .or_else(|| runtime::discover().into_iter().find(|r| r.kind == "wine"));
        let (kind, runner) = default
            .map(|r| (r.kind, r.path))
            .unwrap_or(("proton".into(), String::new()));
        let name = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| game_name(&executable));
        let mut db = self.connection()?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO games (name,install_path,executable,source,runtime_kind,runtime_path) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(install_path) DO NOTHING", params![name, install.display().to_string(), executable.display().to_string(), source, kind, runner]).map_err(|e| e.to_string())?;
        let id: i64 = tx
            .query_row(
                "SELECT id FROM games WHERE install_path=?1",
                [install.display().to_string()],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let prefix = self.data.join("prefixes").join(format!("game-{id}"));
        tx.execute("INSERT INTO prefixes (game_id,path) SELECT ?1,?2 WHERE NOT EXISTS(SELECT 1 FROM prefixes WHERE game_id=?1)", params![id,prefix.display().to_string()]).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        self.games()?
            .into_iter()
            .find(|g| g.id == id)
            .ok_or("Imported game not found".into())
    }

    pub fn scan(&self, directory: Option<String>) -> Result<ScanReport, String> {
        let roots = if let Some(path) = directory {
            let root = PathBuf::from(path);
            if !root.is_dir() {
                return Err("Choose an existing folder".into());
            }
            vec![root]
        } else {
            discovery::default_roots(&self.data)
        };
        let found = discovery::scan(roots);
        let before = self.games()?.len();
        let discovered = found.executables.len();
        for path in &found.executables {
            let _ = self.import(path, None, "discovered");
        }
        Ok(ScanReport {
            added: self.games()?.len().saturating_sub(before),
            discovered,
            roots: found.roots,
            limited: found.limited,
            skipped_directories: found.skipped_directories,
        })
    }

    pub fn save(&self, settings: GameSettings) -> Result<(), String> {
        let _guard = self.launch_lock.lock().map_err(|e| e.to_string())?;
        let game = self
            .games()?
            .into_iter()
            .find(|g| g.id == settings.id)
            .ok_or("Game not found")?;
        if is_running(&game.launch) || game.tools.iter().any(|t| is_running(&t.launch)) {
            return Err(
                "Close the game and its mod managers before changing launch settings".into(),
            );
        }
        if settings.name.trim().is_empty() {
            return Err("A game name is required".into());
        }
        if !["wine", "proton"].contains(&settings.runtime_kind.as_str()) {
            return Err("Choose Wine or Proton".into());
        }
        if !settings.runtime_path.is_empty() {
            which::which(&settings.runtime_path)
                .map_err(|e| format!("Runner must be an executable file: {e}"))?;
        }
        if !display::MODES.contains(&settings.display_mode.as_str()) {
            return Err("Choose a display mode".into());
        }
        if settings.display_mode != "default"
            && (settings.display_width.is_none() || settings.display_height.is_none())
        {
            return Err("Choose a resolution for this display mode".into());
        }
        let db = self.connection()?;
        let updated = db.execute("UPDATE games SET name=?1,runtime_kind=?2,runtime_path=?3,arguments_json=?4,display_mode=?5,display_width=?6,display_height=?7 WHERE id=?8", params![settings.name.trim(),settings.runtime_kind,settings.runtime_path,serde_json::to_string(&settings.arguments).map_err(|e| e.to_string())?,settings.display_mode,settings.display_width,settings.display_height,settings.id]).map_err(|e| e.to_string())?;
        if updated != 1 {
            return Err("Game not found".into());
        }
        Ok(())
    }

    pub fn launch(&self, id: i64) -> Result<Launch, String> {
        self.launch_target(id, None)
    }

    pub fn launch_tool(&self, tool_id: i64) -> Result<Launch, String> {
        let tool = self.find_tool(tool_id)?;
        self.launch_target(tool.game_id, Some(tool_id))
    }

    fn launch_target(&self, id: i64, tool_id: Option<i64>) -> Result<Launch, String> {
        let _guard = self.launch_lock.lock().map_err(|e| e.to_string())?;
        let mut game = self
            .games()?
            .into_iter()
            .find(|g| g.id == id)
            .ok_or("Game not found")?;
        // Display settings belong to the game even when SKSE or a manager starts it.
        let game_executable = PathBuf::from(&game.executable);
        let script_active = game
            .tools
            .iter()
            .any(|t| t.kind == "script_extender" && is_running(&t.launch));
        let mut counts_as_play = tool_id.is_none();
        let working_directory = if let Some(tool_id) = tool_id {
            let tool = game
                .tools
                .iter()
                .find(|t| t.id == tool_id)
                .ok_or("Mod manager profile not found")?;
            if is_running(&tool.launch) {
                return Err("This companion app is already running".into());
            }
            counts_as_play = tool.kind == "script_extender";
            if counts_as_play && (script_active || is_running(&game.launch)) {
                return Err("The game is already running".into());
            }
            let directory = tool.working_directory.clone();
            game.name = tool.name.clone();
            game.executable = tool.executable.clone();
            game.arguments = tool.arguments.clone();
            Some(directory)
        } else {
            if is_running(&game.launch) || script_active {
                return Err("This game is already running".into());
            }
            None
        };
        validate_executable(Path::new(&game.executable))?;
        if game.prefix.is_empty() {
            return Err("No prefix is assigned to this game".into());
        }
        let mut command = launch_command(&game)?;
        if let Some(directory) = working_directory {
            if !Path::new(&directory).is_dir() {
                return Err("The mod manager's working folder is missing".into());
            }
            command.current_dir(directory);
        }
        fs::create_dir_all(&game.prefix).map_err(|e| e.to_string())?;
        let db = self.connection()?;
        let mut log_dir = self.logs.join(format!("game-{id}"));
        if let Some(tool_id) = tool_id {
            log_dir = log_dir.join(format!("tool-{tool_id}"));
        }
        fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let log_path = log_dir.join(format!("launch-{stamp}.log"));
        let mut log = File::create_new(&log_path).map_err(|e| e.to_string())?;
        writeln!(
            log,
            "Respawn+ launch: {}\nExecutable: {}\nRuntime: {}\nPrefix: {}\n",
            game.name, game.executable, game.runtime_path, game.prefix
        )
        .map_err(|e| e.to_string())?;
        if counts_as_play {
            let applied = display::apply(
                Path::new(&game.prefix),
                &game.runtime_kind,
                &game_executable,
                &game.display_mode,
                game.display_width.unwrap_or(0),
                game.display_height.unwrap_or(0),
            )?;
            if let Some(note) = applied {
                writeln!(log, "Display: {note}\n").map_err(|e| e.to_string())?;
            }
        }
        // No PROTON_LOG: companions inherit the first app's environment in a shared container and
        // would reopen (truncate) its steam-<id>.log, destroying the game's crash output.
        command
            .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
            .stderr(Stdio::from(log))
            .stdin(Stdio::null());
        db.execute(
            "INSERT INTO launches (game_id,tool_id,state,log_path) VALUES (?1,?2,'starting',?3)",
            params![id, tool_id, log_path.display().to_string()],
        )
        .map_err(|e| e.to_string())?;
        let launch_id = db.last_insert_rowid();
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let message = format!("Could not start the runtime: {error}");
                db.execute("UPDATE launches SET state='failed',error=?1,ended_at=CURRENT_TIMESTAMP WHERE id=?2", params![message,launch_id]).map_err(|e| e.to_string())?;
                return Err(message);
            }
        };
        let pid = child.id();
        let token = process_token(pid);
        db.execute(
            "UPDATE launches SET state='running',pid=?1,process_token=?2 WHERE id=?3",
            params![pid, token, launch_id],
        )
        .map_err(|e| e.to_string())?;
        if counts_as_play {
            db.execute(
                "UPDATE games SET last_played_at=CURRENT_TIMESTAMP WHERE id=?1",
                [id],
            )
            .map_err(|e| e.to_string())?;
        }
        let path = self.db.clone();
        std::thread::spawn(move || {
            let outcome = child.wait();
            if let Ok(db) = database::open(&path) {
                let (state, code, error) = match outcome {
                    Ok(status) if status.success() => ("finished", status.code(), None),
                    Ok(status) => (
                        "failed",
                        status.code(),
                        Some(format!(
                            "Runtime exited with {status}. Check the launch log."
                        )),
                    ),
                    Err(error) => ("unknown", None, Some(error.to_string())),
                };
                let _ = db.execute("UPDATE launches SET state=?1,exit_code=?2,error=?3,ended_at=CURRENT_TIMESTAMP WHERE id=?4", params![state,code,error,launch_id]);
            }
        });
        Ok(Launch {
            id: launch_id,
            state: "running".into(),
            exit_code: None,
            log_path: log_path.display().to_string(),
            error: None,
        })
    }

    pub fn log(&self, id: i64) -> Result<String, String> {
        let db = self.connection()?;
        let path: String = db
            .query_row(
                "SELECT log_path FROM launches WHERE game_id=?1 AND tool_id IS NULL ORDER BY id DESC LIMIT 1",
                [id],
                |row| row.get(0),
            )
            .map_err(|_| "No launch log yet")?;
        read_log(&path)
    }

    fn find_tool(&self, id: i64) -> Result<Tool, String> {
        self.games()?
            .into_iter()
            .flat_map(|g| g.tools)
            .find(|t| t.id == id)
            .ok_or("Mod manager profile not found".into())
    }

    pub fn save_tool(&self, settings: ToolSettings) -> Result<Tool, String> {
        let _guard = self.launch_lock.lock().map_err(|e| e.to_string())?;
        if settings.name.trim().is_empty() {
            return Err("A companion app name is required".into());
        }
        if !["mod_manager", "script_extender", "trainer", "utility"]
            .contains(&settings.kind.as_str())
        {
            return Err("Choose a supported companion app type".into());
        }
        if !self.games()?.iter().any(|g| g.id == settings.game_id) {
            return Err("Game not found".into());
        }
        let executable = validate_executable(Path::new(&settings.executable))?;
        let directory = if settings.working_directory.trim().is_empty() {
            executable
                .parent()
                .ok_or("Executable has no parent folder")?
                .to_path_buf()
        } else {
            PathBuf::from(&settings.working_directory)
                .canonicalize()
                .map_err(|e| format!("Working folder is unavailable: {e}"))?
        };
        if !directory.is_dir() {
            return Err("Choose an existing working folder".into());
        }
        let db = self.connection()?;
        let arguments = serde_json::to_string(&settings.arguments).map_err(|e| e.to_string())?;
        let id = if let Some(id) = settings.id {
            let old = self.find_tool(id)?;
            if old.game_id != settings.game_id {
                return Err("This profile belongs to another game".into());
            }
            if is_running(&old.launch) {
                return Err("Close the mod manager before editing its profile".into());
            }
            db.execute("UPDATE tools SET name=?1,executable=?2,arguments_json=?3,working_directory=?4,kind=?5 WHERE id=?6", params![settings.name.trim(), executable.display().to_string(), arguments, directory.display().to_string(), settings.kind, id]).map_err(|e| e.to_string())?;
            id
        } else {
            db.execute("INSERT INTO tools (game_id,name,executable,kind,arguments_json,working_directory) VALUES (?1,?2,?3,?4,?5,?6)", params![settings.game_id, settings.name.trim(), executable.display().to_string(), settings.kind, arguments, directory.display().to_string()]).map_err(|e| e.to_string())?;
            db.last_insert_rowid()
        };
        self.find_tool(id)
    }

    pub fn remove_tool(&self, id: i64) -> Result<(), String> {
        let _guard = self.launch_lock.lock().map_err(|e| e.to_string())?;
        let tool = self.find_tool(id)?;
        if is_running(&tool.launch) {
            return Err("Close the mod manager before removing its profile".into());
        }
        self.connection()?
            .execute("DELETE FROM tools WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn tool_log(&self, id: i64) -> Result<String, String> {
        self.find_tool(id)?;
        let path: String = self
            .connection()?
            .query_row(
                "SELECT log_path FROM launches WHERE tool_id=?1 ORDER BY id DESC LIMIT 1",
                [id],
                |r| r.get(0),
            )
            .map_err(|_| "No mod manager launch log yet")?;
        read_log(&path)
    }
}

fn is_running(launch: &Option<Launch>) -> bool {
    launch.as_ref().is_some_and(|l| l.state == "running")
}

/// Play sessions are direct game launches and script-extender launches. Durations come from the
/// supervisor's recorded end time; a running session counts up to now; `unknown` rows were closed
/// by reconciliation long after the fact, so their duration is not trusted.
const PLAY_SESSIONS: &str = "FROM launches l LEFT JOIN tools t ON t.id=l.tool_id WHERE l.game_id=?1 AND (l.tool_id IS NULL OR t.kind='script_extender')";
const SESSION_SECONDS: &str = "CASE WHEN l.state IN ('finished','failed') AND l.ended_at IS NOT NULL THEN CAST(ROUND((julianday(l.ended_at)-julianday(l.started_at))*86400) AS INTEGER) WHEN l.state IN ('starting','running') THEN CAST(ROUND((julianday('now')-julianday(l.started_at))*86400) AS INTEGER) END";
const RECENT_SESSIONS: usize = 8;

fn sessions_for_game(
    db: &Connection,
    game_id: i64,
) -> rusqlite::Result<(i64, Vec<Session>, Vec<DayPlay>)> {
    let total: i64 = db.query_row(
        &format!("SELECT COALESCE(SUM({SESSION_SECONDS}),0) {PLAY_SESSIONS}"),
        [game_id],
        |r| r.get(0),
    )?;
    let mut stmt = db.prepare(&format!(
        "SELECT l.started_at,l.ended_at,l.state,{SESSION_SECONDS},t.name {PLAY_SESSIONS} ORDER BY l.started_at DESC, l.id DESC LIMIT {RECENT_SESSIONS}"
    ))?;
    let sessions = stmt
        .query_map([game_id], |r| {
            Ok(Session {
                started_at: r.get(0)?,
                ended_at: r.get(1)?,
                state: r.get(2)?,
                seconds: r.get(3)?,
                via: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut stmt = db.prepare(&format!(
        "SELECT date(l.started_at),SUM({SESSION_SECONDS}) {PLAY_SESSIONS} AND l.started_at>=date('now','-13 days') GROUP BY 1 HAVING SUM({SESSION_SECONDS})>0 ORDER BY 1"
    ))?;
    let daily = stmt
        .query_map([game_id], |r| {
            Ok(DayPlay {
                date: r.get(0)?,
                seconds: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok((total, sessions, daily))
}

fn tools_for_game(db: &Connection, game_id: i64) -> rusqlite::Result<Vec<Tool>> {
    let mut statement = db.prepare("SELECT t.id,t.game_id,t.name,t.executable,t.arguments_json,t.working_directory,l.id,l.state,l.exit_code,l.log_path,l.error,t.kind FROM tools t LEFT JOIN launches l ON l.id=(SELECT MAX(id) FROM launches WHERE tool_id=t.id) WHERE t.game_id=?1 ORDER BY t.name COLLATE NOCASE")?;
    let rows = statement.query_map([game_id], |r| {
        let executable: String = r.get(3)?;
        let mut working_directory: String = r.get(5)?;
        if working_directory.is_empty() {
            working_directory = Path::new(&executable)
                .parent()
                .unwrap_or(Path::new("."))
                .display()
                .to_string();
        }
        let launch = match r.get::<_, Option<i64>>(6)? {
            Some(id) => Some(Launch {
                id,
                state: r.get(7)?,
                exit_code: r.get(8)?,
                log_path: r.get(9)?,
                error: r.get(10)?,
            }),
            None => None,
        };
        Ok(Tool {
            id: r.get(0)?,
            game_id: r.get(1)?,
            kind: r.get(11)?,
            name: r.get(2)?,
            available: Path::new(&executable).is_file(),
            executable,
            arguments: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
            working_directory,
            launch,
        })
    })?;
    rows.collect()
}

fn read_log(path: &str) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(64 * 1024)))
        .map_err(|e| e.to_string())?;
    let mut bytes = vec![];
    file.take(64 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn validate_executable(path: &Path) -> Result<PathBuf, String> {
    let path = path
        .canonicalize()
        .map_err(|e| format!("Cannot open game executable: {e}"))?;
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("exe"))
    {
        return Err("Choose a Windows .exe file".into());
    }
    let mut signature = [0; 2];
    File::open(&path)
        .and_then(|mut f| f.read_exact(&mut signature))
        .map_err(|e| e.to_string())?;
    if signature != *b"MZ" {
        return Err("The file is not a Windows executable".into());
    }
    Ok(path)
}

fn game_name(path: &Path) -> String {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    match stem.to_lowercase().as_str() {
        "skyrimse" => "Skyrim Special Edition".into(),
        "mines" => "Mines".into(),
        _ => stem.replace("-Win64-Shipping", "").replace('_', " "),
    }
}

fn launch_command(game: &Game) -> Result<Command, String> {
    if game.runtime_path.is_empty() {
        return Err("Select a Wine or Proton runtime for this game first".into());
    }
    let runner = which::which(&game.runtime_path)
        .map_err(|e| format!("Runtime is unavailable: {e}"))?
        .canonicalize()
        .map_err(|e| format!("Runtime path is unavailable: {e}"))?;
    let mut command = if game.runtime_kind == "proton" {
        let umu = runtime::discover()
            .into_iter()
            .find(|r| r.kind == "umu")
            .ok_or("UMU is required to launch Proton games outside Steam")?;
        proton_command(Path::new(&umu.path), &runner)?
    } else if game.runtime_kind == "wine" {
        Command::new(runner)
    } else {
        return Err("Unsupported runtime type".into());
    };
    command
        .arg(&game.executable)
        .args(&game.arguments)
        .env("WINEPREFIX", &game.prefix)
        .current_dir(
            Path::new(&game.executable)
                .parent()
                .ok_or("Game has no installation directory")?,
        );
    Ok(command)
}

fn proton_command(umu: &Path, runner: &Path) -> Result<Command, String> {
    let mut command = Command::new(umu);
    command
        .env(
            "PROTONPATH",
            runner.parent().ok_or("Proton has no parent directory")?,
        )
        .env("GAMEID", "0")
        // UMU keys the container's launcher service by canonical prefix path.
        // The first app creates it; subsequent apps enter that same container
        // using runinprefix. Do not guess readiness from our launch records.
        .env("UMU_CONTAINER_NSENTER", "1")
        // Waiting for the entire Wine session before starting would serialize
        // the game and its companion apps.
        .env("PROTON_VERB", "run")
        .env("UMU_LOG", "info");
    Ok(command)
}

fn process_token(pid: u32) -> Option<String> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<_> = text
        .get(text.rfind(") ")? + 2..)?
        .split_whitespace()
        .collect();
    if fields.first() == Some(&"Z") {
        return None;
    }
    fields.get(19).map(|s| s.to_string())
}

fn reconcile(db: &Connection) -> Result<(), String> {
    let mut stmt = db
        .prepare("SELECT id,pid,process_token FROM launches WHERE state IN ('starting','running')")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<u32>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        // Finish reading before writing: upgrading an open WAL read snapshot to a write fails
        // immediately (no busy wait) if a launch supervisor committed in between.
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    for (id, pid, token) in rows {
        if !pid
            .zip(token)
            .is_some_and(|(pid, token)| process_token(pid).as_deref() == Some(&token))
        {
            db.execute("UPDATE launches SET state='unknown',ended_at=CURRENT_TIMESTAMP WHERE id=?1 AND state IN ('starting','running')", [id]).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn proton_launches_request_container_reentry_and_never_wait_for_other_apps() {
        let temp = tempfile::tempdir().unwrap();
        let umu = temp.path().join("umu test");
        // Exercise the child environment passed to UMU. Real container sharing is a separate GUI
        // integration check; this stub only checks our UMU invocation contract.
        fs::write(&umu, "#!/bin/sh\nprintf '%s\\n' \"$UMU_CONTAINER_NSENTER\" \"$PROTON_VERB\" \"$PROTONPATH\" \"$WINEPREFIX\" \"$@\"\n").unwrap();
        fs::set_permissions(&umu, fs::Permissions::from_mode(0o755)).unwrap();
        let proton = temp.path().join("GE Proton/proton");
        let prefix = temp.path().join("prefix with spaces");
        let mut command = proton_command(&umu, &proton).unwrap();
        command
            .env("WINEPREFIX", &prefix)
            .arg("Trainer with spaces.exe")
            .arg("literal ; $value & text");
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            vec![
                "1",
                "run",
                proton.parent().unwrap().to_str().unwrap(),
                prefix.to_str().unwrap(),
                "Trainer with spaces.exe",
                "literal ; $value & text"
            ]
        );
    }

    #[test]
    fn mod_managers_share_the_game_environment_and_have_independent_history() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::at(temp.path().join("state"), temp.path().join("logs")).unwrap();
        let exe = temp.path().join("Game.exe");
        fs::write(&exe, b"MZtest").unwrap();
        let manager = temp.path().join("Manager with spaces.exe");
        fs::write(&manager, b"MZtest").unwrap();
        let working = temp.path().join("manager working folder");
        fs::create_dir(&working).unwrap();
        let runner = temp.path().join("wine-test");
        fs::write(
            &runner,
            "#!/bin/sh\nprintf '%s\\n' \"$PWD\" \"$WINEPREFIX\" \"$@\"\nsleep 0.4\nexit 7\n",
        )
        .unwrap();
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
        let game = launcher.import(&exe, None, "manual").unwrap();
        launcher
            .save(GameSettings {
                display_mode: "default".into(),
                display_width: None,
                display_height: None,
                id: game.id,
                name: game.name.clone(),
                runtime_kind: "wine".into(),
                runtime_path: runner.display().to_string(),
                arguments: vec![],
            })
            .unwrap();
        let profile = launcher
            .save_tool(ToolSettings {
                id: None,
                game_id: game.id,
                kind: default_tool_kind(),
                name: "Test manager".into(),
                executable: manager.display().to_string(),
                arguments: vec!["literal space; $value".into()],
                working_directory: working.display().to_string(),
            })
            .unwrap();
        launcher.launch_tool(profile.id).unwrap();
        assert!(launcher.launch_tool(profile.id).is_err());
        assert!(launcher.remove_tool(profile.id).is_err());
        assert!(launcher
            .save(GameSettings {
                display_mode: "default".into(),
                display_width: None,
                display_height: None,
                id: game.id,
                name: game.name.clone(),
                runtime_kind: "wine".into(),
                runtime_path: runner.display().to_string(),
                arguments: vec![]
            })
            .is_err());
        let snapshot = launcher.games().unwrap().remove(0);
        assert!(snapshot.launch.is_none());
        assert!(snapshot.last_played_at.is_none());
        assert!(is_running(&snapshot.tools[0].launch));
        // A game may be opened while its companion app is in the same prefix.
        launcher.launch(game.id).unwrap();
        for _ in 0..100 {
            let profile = launcher.find_tool(profile.id).unwrap();
            if profile
                .launch
                .as_ref()
                .is_some_and(|l| l.exit_code == Some(7))
            {
                let log = launcher.tool_log(profile.id).unwrap();
                assert!(log.contains(&game.prefix));
                assert!(log.contains(&working.display().to_string()));
                assert!(log.contains("literal space; $value"));
                assert!(log.contains("Manager with spaces.exe"));
                assert!(!launcher
                    .log(game.id)
                    .unwrap()
                    .contains("Manager with spaces.exe"));
                launcher.remove_tool(profile.id).unwrap();
                assert!(manager.is_file());
                assert!(launcher.find_tool(profile.id).is_err());
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("Manager exit was not recorded");
    }

    #[test]
    fn manager_profiles_persist_and_reject_cross_game_edits() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("state");
        let logs = temp.path().join("logs");
        let launcher = Launcher::at(data.clone(), logs.clone()).unwrap();
        fs::create_dir(temp.path().join("other")).unwrap();
        let exe = temp.path().join("Game.exe");
        fs::write(&exe, b"MZtest").unwrap();
        let other = temp.path().join("other/Game.exe");
        fs::write(&other, b"MZtest").unwrap();
        let first = launcher.import(&exe, None, "manual").unwrap();
        let second = launcher.import(&other, None, "manual").unwrap();
        let profile = launcher
            .save_tool(ToolSettings {
                id: None,
                game_id: first.id,
                kind: default_tool_kind(),
                name: "MO2".into(),
                executable: exe.display().to_string(),
                arguments: vec!["-p".into(), "Modded profile".into()],
                working_directory: String::new(),
            })
            .unwrap();
        assert!(launcher
            .save_tool(ToolSettings {
                id: Some(profile.id),
                game_id: second.id,
                kind: default_tool_kind(),
                name: "Moved".into(),
                executable: exe.display().to_string(),
                arguments: vec![],
                working_directory: String::new()
            })
            .is_err());
        drop(launcher);
        let launcher = Launcher::at(data, logs).unwrap();
        let restored = launcher.find_tool(profile.id).unwrap();
        assert_eq!(restored.arguments, vec!["-p", "Modded profile"]);
        assert_eq!(restored.game_id, first.id);
        assert_eq!(
            restored.working_directory,
            temp.path().display().to_string()
        );
        assert!(launcher.tool_log(profile.id).is_err());
    }

    #[test]
    fn import_is_idempotent_and_preserves_custom_settings_after_rescan() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::at(temp.path().join("state"), temp.path().join("logs")).unwrap();
        let exe = temp.path().join("Mines.exe");
        fs::write(&exe, b"MZtest").unwrap();
        let game = launcher.import(&exe, None, "manual").unwrap();
        launcher
            .save(GameSettings {
                display_mode: "default".into(),
                display_width: None,
                display_height: None,
                id: game.id,
                name: "My Mines".into(),
                runtime_kind: "wine".into(),
                runtime_path: String::new(),
                arguments: vec!["two words".into()],
            })
            .unwrap();
        launcher.import(&exe, None, "discovered").unwrap();
        let games = launcher.games().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "My Mines");
        assert_eq!(games[0].arguments, vec!["two words"]);
        assert!(launcher.import(temp.path(), None, "manual").is_err());
    }

    #[test]
    fn launches_with_separate_prefix_literal_arguments_and_records_exit() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::at(temp.path().join("state"), temp.path().join("logs")).unwrap();
        let exe = temp.path().join("Game with spaces.exe");
        fs::write(&exe, b"MZtest").unwrap();
        let runner = temp.path().join("wine-test");
        fs::write(
            &runner,
            "#!/bin/sh\nprintf '%s\\n' \"$WINEPREFIX\" \"$@\"\nsleep 0.3\nexit 7\n",
        )
        .unwrap();
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
        let game = launcher.import(&exe, None, "manual").unwrap();
        launcher
            .save(GameSettings {
                display_mode: "default".into(),
                display_width: None,
                display_height: None,
                id: game.id,
                name: game.name.clone(),
                runtime_kind: "wine".into(),
                runtime_path: runner.display().to_string(),
                arguments: vec!["two words; literal $value".into()],
            })
            .unwrap();
        launcher.launch(game.id).unwrap();
        assert!(launcher.launch(game.id).is_err());
        for _ in 0..100 {
            let latest = launcher.games().unwrap().remove(0);
            if latest
                .launch
                .as_ref()
                .is_some_and(|l| l.exit_code == Some(7))
            {
                let log = launcher.log(game.id).unwrap();
                assert!(log.contains("two words; literal $value"));
                assert!(log.contains(&game.prefix));
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("Launch exit was not recorded");
    }

    #[test]
    fn runtime_output_stays_in_the_launch_log_instead_of_a_shared_proton_log() {
        // Companions joining a shared container inherit the first app's environment. Proton's
        // own steam-<id>.log is reopened for writing by every app that joins, so the game's crash
        // output would be truncated by a trainer. Keep each app's output in its own launch log.
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::at(temp.path().join("state"), temp.path().join("logs")).unwrap();
        let exe = temp.path().join("Game.exe");
        fs::write(&exe, b"MZtest").unwrap();
        let runner = temp.path().join("wine-test");
        fs::write(
            &runner,
            "#!/bin/sh\necho \"proton-log=[$PROTON_LOG] dir=[$PROTON_LOG_DIR]\"\n",
        )
        .unwrap();
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
        let game = launcher.import(&exe, None, "manual").unwrap();
        launcher
            .save(GameSettings {
                display_mode: "default".into(),
                display_width: None,
                display_height: None,
                id: game.id,
                name: game.name.clone(),
                runtime_kind: "wine".into(),
                runtime_path: runner.display().to_string(),
                arguments: vec![],
            })
            .unwrap();
        launcher.launch(game.id).unwrap();
        for _ in 0..100 {
            let log = launcher.log(game.id).unwrap();
            if log.contains("proton-log=") {
                assert!(log.contains("proton-log=[] dir=[]"), "{log}");
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("Runtime output was not written to the launch log");
    }

    #[test]
    fn play_time_counts_game_and_script_extender_sessions_with_known_durations() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::at(temp.path().join("state"), temp.path().join("logs")).unwrap();
        let exe = temp.path().join("Game.exe");
        fs::write(&exe, b"MZtest").unwrap();
        let game = launcher.import(&exe, None, "manual").unwrap();
        let db = launcher.connection().unwrap();
        db.execute("INSERT INTO tools (game_id,name,executable,kind,arguments_json,environment_json,working_directory) VALUES (?1,'SKSE','/skse.exe','script_extender','[]','{}','/')", [game.id]).unwrap();
        let skse = db.last_insert_rowid();
        db.execute("INSERT INTO tools (game_id,name,executable,kind,arguments_json,environment_json,working_directory) VALUES (?1,'Trainer','/t.exe','trainer','[]','{}','/')", [game.id]).unwrap();
        let trainer = db.last_insert_rowid();
        for (tool, state, start, end) in [
            (
                None,
                "finished",
                "2026-10-24 10:00:00",
                Some("2026-10-24 10:30:00"),
            ),
            (
                Some(skse),
                "failed",
                "2026-10-24 11:00:00",
                Some("2026-10-24 12:00:00"),
            ),
            (
                Some(trainer),
                "finished",
                "2026-10-24 11:05:00",
                Some("2026-10-24 11:55:00"),
            ),
            (
                None,
                "unknown",
                "2026-10-24 13:00:00",
                Some("2026-10-24 18:00:00"),
            ),
        ] {
            db.execute("INSERT INTO launches (game_id,tool_id,state,log_path,started_at,ended_at) VALUES (?1,?2,?3,'/log',?4,?5)", params![game.id, tool, state, start, end]).unwrap();
        }
        let game = launcher.games().unwrap().remove(0);
        // 30 min direct + 60 min via SKSE; the trainer is not play time; unknown has no reliable end.
        assert_eq!(game.play_seconds, 90 * 60);
        let sessions: Vec<_> = game
            .sessions
            .iter()
            .map(|s| (s.started_at.as_str(), s.seconds, s.via.as_deref()))
            .collect();
        assert_eq!(
            sessions,
            vec![
                ("2026-10-24 13:00:00", None, None),
                ("2026-10-24 11:00:00", Some(3600), Some("SKSE")),
                ("2026-10-24 10:00:00", Some(1800), None),
            ]
        );
    }

    #[test]
    fn script_extender_launch_applies_the_game_display_mode_before_starting() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::at(temp.path().join("state"), temp.path().join("logs")).unwrap();
        let folder = temp.path().join("Skyrim");
        fs::create_dir_all(&folder).unwrap();
        let exe = folder.join("SkyrimSE.exe");
        let loader = folder.join("skse64_loader.exe");
        fs::write(&exe, b"MZtest").unwrap();
        fs::write(&loader, b"MZtest").unwrap();
        let runner = temp.path().join("wine-test");
        fs::write(&runner, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
        let game = launcher.import(&exe, None, "manual").unwrap();
        launcher
            .save(GameSettings {
                id: game.id,
                name: game.name.clone(),
                runtime_kind: "wine".into(),
                runtime_path: runner.display().to_string(),
                arguments: vec![],
                display_mode: "borderless".into(),
                display_width: Some(2560),
                display_height: Some(1440),
            })
            .unwrap();
        let skse = launcher
            .save_tool(ToolSettings {
                id: None,
                game_id: game.id,
                kind: "script_extender".into(),
                name: "SKSE".into(),
                executable: loader.display().to_string(),
                arguments: vec![],
                working_directory: String::new(),
            })
            .unwrap();
        let launch = launcher.launch_tool(skse.id).unwrap();
        let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
        let prefs = Path::new(&game.prefix)
            .join("drive_c/users")
            .join(user)
            .join("Documents/My Games/Skyrim Special Edition/SkyrimPrefs.ini");
        assert_eq!(
            fs::read_to_string(prefs).unwrap(),
            "[Display]\nbFull Screen=0\nbBorderless=1\niSize W=2560\niSize H=1440\n"
        );
        assert!(read_log(&launch.log_path)
            .unwrap()
            .contains("Display: borderless 2560×1440"));
    }

    #[test]
    fn daily_play_covers_the_last_fourteen_days_including_older_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::at(temp.path().join("state"), temp.path().join("logs")).unwrap();
        let exe = temp.path().join("Game.exe");
        fs::write(&exe, b"MZtest").unwrap();
        let game = launcher.import(&exe, None, "manual").unwrap();
        let db = launcher.connection().unwrap();
        // Ten 10-minute sessions yesterday: more than the session list keeps, all in the daily total.
        for _ in 0..10 {
            db.execute("INSERT INTO launches (game_id,state,log_path,started_at,ended_at) VALUES (?1,'finished','/log',datetime('now','-1 day','start of day','+12 hours'),datetime('now','-1 day','start of day','+12 hours','+10 minutes'))", [game.id]).unwrap();
        }
        db.execute("INSERT INTO launches (game_id,state,log_path,started_at,ended_at) VALUES (?1,'finished','/log',datetime('now','-30 days'),datetime('now','-30 days','+1 hour'))", [game.id]).unwrap();
        let game = launcher.games().unwrap().remove(0);
        let yesterday: String = db
            .query_row("SELECT date('now','-1 day')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(game.sessions.len(), 8);
        assert_eq!(
            game.daily_play,
            vec![DayPlay {
                date: yesterday,
                seconds: 6000
            }]
        );
        assert_eq!(game.play_seconds, 6000 + 3600);
    }
}
