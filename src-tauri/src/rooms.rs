// SPDX-License-Identifier: MIT
//! A room owns a persistent bubblewrap namespace; each launch is a client of its supervisor.
//! The package store and profile format have no knowledge of game titles or mod managers.
use crate::{
    library::{Game, Launcher},
    recovery,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File},
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

const ENGINE: &str = include_str!("../room_engine/engine.py");

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RoomProfile {
    pub network: String,
    pub display: String,
    pub graphics: bool,
    pub audio: bool,
    pub controllers: bool,
    pub requirements: Vec<String>,
    pub packages: Vec<String>,
    /// Save directories relative to the effective Wine prefix; data-driven for any game.
    pub save_paths: Vec<String>,
    /// Save directories relative to the directory containing the game executable.
    pub game_save_paths: Vec<String>,
    /// Save directories in the room's private Linux home (for redirected Windows folders).
    pub home_save_paths: Vec<String>,
}

impl Default for RoomProfile {
    fn default() -> Self {
        Self {
            network: "online".into(),
            display: "auto".into(),
            graphics: true,
            audio: true,
            controllers: true,
            requirements: Vec::new(),
            packages: Vec::new(),
            save_paths: Vec::new(),
            game_save_paths: Vec::new(),
            home_save_paths: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RoomStatus {
    pub profile: RoomProfile,
    pub active: bool,
}

#[derive(Default)]
pub(crate) struct RoomManager {
    sessions: Mutex<HashMap<i64, Session>>,
    /// Physical size of the monitor showing the launcher, as reported by the host's windowing system.
    monitor: Mutex<Option<(u32, u32)>>,
}

struct Session {
    child: Child,
    socket: PathBuf,
    engine: PathBuf,
    fingerprint: String,
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            // The host engine forwards TERM to its pasta/bwrap process group and reaps it.
            unsafe {
                libc::kill(self.child.id() as i32, libc::SIGTERM);
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                if self.child.try_wait().ok().flatten().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(directory) = self.socket.parent() {
            let _ = fs::remove_dir_all(directory);
        }
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn python(engine: &Path, action: &str) -> Command {
    let mut command = Command::new("/usr/bin/python3");
    command.arg("-I").arg(engine).arg(action);
    command
}

fn engine(data: &Path) -> Result<PathBuf, String> {
    let directory = data.join("libexec");
    fs::create_dir_all(&directory).map_err(err)?;
    let path = directory.join("room-engine.py");
    if fs::read_to_string(&path).ok().as_deref() != Some(ENGINE) {
        let temporary = directory.join("room-engine.new");
        fs::write(&temporary, ENGINE).map_err(err)?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o500)).map_err(err)?;
        fs::rename(temporary, &path).map_err(err)?;
    }
    Ok(path)
}

fn output(command: &mut Command) -> Result<Value, String> {
    let result = command.output().map_err(err)?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).trim().into());
    }
    serde_json::from_slice(&result.stdout).map_err(err)
}

pub fn status(db: &rusqlite::Connection, game_id: i64) -> rusqlite::Result<RoomStatus> {
    let profile = db.query_row(
        "SELECT profile_json FROM room_profiles WHERE game_id=?1",
        [game_id],
        |row| row.get::<_, String>(0),
    );
    let profile = match profile {
        Ok(json) => serde_json::from_str(&json)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
        Err(rusqlite::Error::QueryReturnedNoRows) => RoomProfile::default(),
        Err(e) => return Err(e),
    };
    Ok(RoomStatus {
        profile,
        active: false,
    })
}

fn valid_profile(profile: &RoomProfile) -> Result<(), String> {
    for path in profile
        .save_paths
        .iter()
        .chain(&profile.game_save_paths)
        .chain(&profile.home_save_paths)
    {
        if path.is_empty()
            || path.contains(['\\', '\0', ':'])
            || !Path::new(path)
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return Err(
                "Save mappings must be relative directories within their selected room root".into(),
            );
        }
    }
    if !["online", "offline"].contains(&profile.network.as_str())
        || !["auto", "nested-x11", "wayland", "headless"].contains(&profile.display.as_str())
    {
        return Err("Unsupported room network or display policy".into());
    }
    if profile.requirements.len() > 128
        || profile.packages.len() > 128
        || profile
            .requirements
            .iter()
            .any(|s| s.is_empty() || s.len() > 255)
        || profile.packages.iter().any(|s| {
            s.len() != 64
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
    {
        return Err("Invalid room runtime requirements or pins".into());
    }
    Ok(())
}

/// Size of the room's private X screen. A game that renders larger than this screen leaves the rest
/// of its window undrawn, so it follows the game's own display setting, else the user's monitor.
fn xwayland_screen(game: &Game, monitor: Option<(u32, u32)>) -> Result<Value, String> {
    let fullscreen = game.display_mode != "windowed";
    let size = match (
        game.display_mode.as_str(),
        game.display_width,
        game.display_height,
    ) {
        ("default", ..) | (_, None, _) | (_, _, None) => monitor,
        (_, Some(width), Some(height)) => Some((width, height)),
    };
    let (width, height) = size.ok_or(
        "Cannot determine the monitor size for this room's display. Set a resolution in the game's display settings.",
    )?;
    Ok(json!({"fullscreen":fullscreen,"width":width,"height":height}))
}

fn writable_roots(game: &Game, data: &Path) -> Result<Vec<PathBuf>, String> {
    let prefix = Path::new(&game.prefix).canonicalize().map_err(err)?;
    let data = data.canonicalize().map_err(err)?;
    let home = dirs::home_dir()
        .ok_or("Cannot determine user home")?
        .canonicalize()
        .map_err(err)?;
    let mut roots = vec![
        prefix.clone(),
        Path::new(&game.executable)
            .parent()
            .ok_or("Game directory missing")?
            .canonicalize()
            .map_err(err)?,
    ];
    for tool in &game.tools {
        roots.push(
            Path::new(&tool.executable)
                .parent()
                .ok_or("Companion directory missing")?
                .canonicalize()
                .map_err(err)?,
        );
        roots.push(
            Path::new(&tool.working_directory)
                .canonicalize()
                .map_err(err)?,
        );
    }
    for root in &roots {
        if !root.is_dir()
            || root.components().count() < 3
            || home.starts_with(root)
            || (root != &prefix
                && !root.starts_with(&prefix)
                && (data.starts_with(root) || root.starts_with(&data)))
            || [
                "/usr", "/etc", "/bin", "/sbin", "/lib", "/lib64", "/dev", "/proc", "/sys", "/run",
                "/var", "/boot",
            ]
            .iter()
            .any(|p| root.starts_with(p))
        {
            return Err(format!(
                "{} is too broad or shared for a room. Use a dedicated game or companion folder.",
                root.display()
            ));
        }
    }
    roots.sort_by_key(|root| root.components().count());
    let mut selected: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !selected.iter().any(|r| root.starts_with(r)) {
            selected.push(root);
        }
    }
    Ok(selected)
}

fn command_request(command: &Command) -> Value {
    let argv = std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let environment = command
        .get_envs()
        .filter_map(|(key, value)| {
            value.map(|value| {
                (
                    key.to_string_lossy().into_owned(),
                    value.to_string_lossy().into_owned(),
                )
            })
        })
        .collect::<HashMap<_, _>>();
    json!({"argv": argv, "env": environment, "cwd": command.get_current_dir().unwrap_or(Path::new("/")).to_string_lossy()})
}

fn read_only_root(path: &Path, writable: &[PathBuf]) -> Result<PathBuf, String> {
    let path = path.canonicalize().map_err(err)?;
    if writable
        .iter()
        .any(|root| root.starts_with(&path) || path.starts_with(root))
    {
        return Err("A runner directory overlaps writable room files".into());
    }
    let home = dirs::home_dir().ok_or("Cannot determine home")?;
    if home.starts_with(&path) || path == Path::new("/") {
        return Err("Runner directory is too broad to expose to a room".into());
    }
    Ok(path)
}

/// Small UMU metadata lives in the private home; large preinstalled Linux runtime directories
/// and files are read-only mounts. Locks, caches and pressure-vessel var/ stay room-local.
fn umu_mounts(
    home: &Path,
    guest_home: &Path,
    mounts: &mut Vec<Value>,
    private: &mut Vec<Value>,
) -> Result<(), String> {
    let source = dirs::data_dir().ok_or("Cannot find UMU data")?.join("umu");
    if !source.exists() {
        return Err(
            "The shared UMU Linux runtime is not installed. Run Respawn+ runtime setup first."
                .into(),
        );
    }
    let target = home.join(".local/share/umu");
    fs::create_dir_all(&target).map_err(err)?;
    for entry in fs::read_dir(source).map_err(err)? {
        let entry = entry.map_err(err)?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("steamrt") && entry.path().is_dir() {
            fs::create_dir_all(target.join(&name).join("var")).map_err(err)?;
            for member in fs::read_dir(entry.path()).map_err(err)? {
                let member = member.map_err(err)?;
                if member.file_name() == "var" {
                    continue;
                }
                if member.file_name() == ".installed.ok" {
                    // UMU refreshes this small marker even with runtime updates disabled.
                    // It must be private metadata, not an immutable file bind mount.
                    fs::copy(member.path(), target.join(&name).join(".installed.ok"))
                        .map_err(err)?;
                    continue;
                }
                mounts.push(json!([
                    member.path().canonicalize().map_err(err)?,
                    guest_home
                        .join(".local/share/umu")
                        .join(&name)
                        .join(member.file_name())
                ]));
                if member.path().join("files/.ref").is_file() {
                    let references = home.join(".cache/respawn-runtime-refs");
                    fs::create_dir_all(&references).map_err(err)?;
                    let reference = references.join(format!(
                        "{}-{}",
                        name.to_string_lossy(),
                        member.file_name().to_string_lossy()
                    ));
                    File::options()
                        .write(true)
                        .create(true)
                        .truncate(false)
                        .open(&reference)
                        .map_err(err)?;
                    private.push(json!([
                        reference,
                        guest_home
                            .join(".local/share/umu")
                            .join(&name)
                            .join(member.file_name())
                            .join("files/.ref")
                    ]));
                }
            }
        } else if name == "umu-shim" {
            mounts.push(json!([
                entry.path().canonicalize().map_err(err)?,
                guest_home.join(".local/share/umu/umu-shim")
            ]));
        }
    }
    Ok(())
}

impl RoomManager {
    pub(crate) fn active(&self, game_id: i64) -> bool {
        self.sessions
            .lock()
            .ok()
            .and_then(|mut sessions| {
                sessions
                    .get_mut(&game_id)
                    .map(|session| session.child.try_wait().ok().flatten().is_none())
            })
            .unwrap_or(false)
    }
    fn inspect(&self, game_id: i64, screenshot: bool) -> Result<Value, String> {
        let sessions = self.sessions.lock().map_err(err)?;
        let session = sessions.get(&game_id).ok_or("Room is not open")?;
        output(
            python(&session.engine, "exec")
                .arg(&session.socket)
                .arg(json!({"diagnostics":true,"screenshot":screenshot}).to_string()),
        )
    }
    pub(crate) fn set_monitor(&self, size: Option<(u32, u32)>) -> Result<(), String> {
        *self.monitor.lock().map_err(err)? = size;
        Ok(())
    }

    pub(crate) fn stop(&self, game_id: i64) -> Result<(), String> {
        self.sessions.lock().map_err(err)?.remove(&game_id);
        Ok(())
    }
    pub(crate) fn quiesce(&self, game_id: i64) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(err)?;
        if let Some(session) = sessions.get_mut(&game_id) {
            if session.child.try_wait().map_err(err)?.is_none() {
                let status = python(&session.engine, "exec")
                    .arg(&session.socket)
                    .arg("{\"status\":true}")
                    .stdout(Stdio::null())
                    .status()
                    .map_err(err)?;
                if !status.success() {
                    return Err("This room still has running processes. Close the game and companions before changing its setup.".into());
                }
            }
        }
        sessions.remove(&game_id);
        Ok(())
    }

    fn command(
        &self,
        data: &Path,
        game: &Game,
        profile: &RoomProfile,
        command: Command,
    ) -> Result<Command, String> {
        let engine = engine(data)?;
        let roots = writable_roots(game, data)?;
        let resolved = output(
            python(&engine, "plan")
                .arg(data.join("windows-runtimes"))
                .arg(serde_json::to_string(profile).map_err(err)?)
                .arg(
                    serde_json::to_string(
                        &std::iter::once(&game.executable)
                            .chain(game.tools.iter().map(|tool| &tool.executable))
                            .collect::<Vec<_>>(),
                    )
                    .map_err(err)?,
                ),
        )?;
        let fingerprint = serde_json::to_string(&json!([
            profile,
            resolved,
            game.runtime_path,
            game.runtime_kind,
            game.prefix,
            roots
        ]))
        .map_err(err)?;
        let mut sessions = self.sessions.lock().map_err(err)?;
        if let Some(session) = sessions.get_mut(&game.id) {
            if session.child.try_wait().map_err(err)?.is_some() {
                sessions.remove(&game.id);
            }
        }
        if let std::collections::hash_map::Entry::Vacant(entry) = sessions.entry(game.id) {
            recovery::ensure_idle(game)?;
            let roots = writable_roots(game, data)?;
            let root = data.join("rooms").join(format!("game-{}", game.id));
            let home = root.join("home");
            fs::create_dir_all(&home).map_err(err)?;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(err)?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(err)?
                .as_nanos();
            // Unix socket names are limited to ~108 bytes; app-data paths can be much longer.
            let ipc = std::env::temp_dir().join(format!(
                "respawn-{}-{}-{stamp:x}",
                std::process::id(),
                game.id
            ));
            std::os::unix::fs::DirBuilderExt::mode(&mut fs::DirBuilder::new(), 0o700)
                .create(&ipc)
                .map_err(err)?;
            let guest_home = dirs::home_dir().ok_or("Cannot determine home")?;
            let uid = unsafe { libc::getuid() };
            let user = std::env::var("USER")
                .ok()
                .filter(|s| {
                    !s.is_empty()
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                })
                .unwrap_or_else(|| format!("room{uid}"));
            let accounts = root.join("passwd");
            fs::write(
                &accounts,
                format!(
                    "root:x:0:0:root:/root:/bin/false\n{user}:x:{uid}:{}:Respawn room:{}:/bin/sh\n",
                    unsafe { libc::getgid() },
                    guest_home.display()
                ),
            )
            .map_err(err)?;
            let display = if profile.display == "auto" {
                if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                    "nested-x11"
                } else if std::env::var_os("DISPLAY").is_some() {
                    return Err("Secure rooms need Wayland for a private game display. Host X11 access is not granted automatically.".into());
                } else {
                    "headless"
                }
            } else {
                &profile.display
            };
            let host_runtime = std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(format!("/run/user/{uid}")));
            let wayland = host_runtime
                .join(std::env::var_os("WAYLAND_DISPLAY").unwrap_or_else(|| "wayland-0".into()));
            if display != "headless" && !wayland.exists() {
                return Err("The Wayland display required by this room is unavailable".into());
            }
            let runner = which::which(&game.runtime_path)
                .map_err(err)?
                .canonicalize()
                .map_err(err)?;
            let mut read_only = Vec::new();
            if !runner.starts_with("/usr") {
                read_only.push(read_only_root(
                    runner.parent().ok_or("Missing runner directory")?,
                    &roots,
                )?);
            }
            let mut runtime_mounts = Vec::new();
            let mut private_mounts = Vec::new();
            if game.runtime_kind == "proton" {
                let umu = PathBuf::from(command.get_program())
                    .canonicalize()
                    .map_err(err)?;
                if !umu.starts_with("/usr") {
                    read_only.push(read_only_root(
                        umu.parent().ok_or("Missing UMU directory")?,
                        &roots,
                    )?);
                }
                umu_mounts(&home, &guest_home, &mut runtime_mounts, &mut private_mounts)?;
            }
            let mut environment = command_request(&command)["env"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            if game.runtime_kind == "proton" {
                environment.insert(
                    "PRESSURE_VESSEL_FILESYSTEMS_RO".into(),
                    Value::String("/run/respawn/packages".into()),
                );
            }
            // Only explicit runner variables are carried over; no host environment inheritance.
            let overrides = resolved["dll_overrides"]
                .as_object()
                .ok_or("Invalid runtime resolution")?;
            if !overrides.is_empty() {
                environment.insert(
                    "WINEDLLOVERRIDES".into(),
                    Value::String(
                        overrides
                            .iter()
                            .map(|(dll, order)| {
                                format!("{dll}={}", order.as_str().unwrap_or_default())
                            })
                            .collect::<Vec<_>>()
                            .join(";"),
                    ),
                );
            }
            let guest_config = root.join(format!("supervisor-{stamp}.json"));
            let config_path = root.join(format!("session-{stamp}.json"));
            let screen = if display == "nested-x11" {
                let monitor = *self.monitor.lock().map_err(err)?;
                xwayland_screen(game, monitor)?
            } else {
                Value::Null
            };
            let config = json!({"display":display,"screen":screen,"once":false,"private_bus":game.runtime_kind=="proton","registry_commands":[]});
            fs::write(&guest_config, serde_json::to_vec(&config).map_err(err)?).map_err(err)?;
            let mut host_config = json!({"network":profile.network,"display":display,"screen":screen,"graphics":profile.graphics,"audio":profile.audio,"controllers":profile.controllers,"wayland":wayland,"user":user,"home":home,"guest_home":guest_home,"accounts":accounts,"engine":engine,"guest_config":guest_config,"ipc":ipc,"writable_roots":roots,"read_only_roots":read_only,"runtime_mounts":runtime_mounts,"private_mounts":private_mounts,"prefix":game.prefix,"environment":environment});
            let packages = resolved["packages"]
                .as_array()
                .ok_or("Invalid runtime package resolution")?;
            if !packages.is_empty() {
                Self::provision(
                    &engine,
                    &root,
                    &mut host_config,
                    &guest_config,
                    game,
                    &command,
                    packages,
                )?;
            }
            fs::write(&config_path, serde_json::to_vec(&host_config).map_err(err)?).map_err(err)?;
            let log = File::create(root.join("room.log")).map_err(err)?;
            let mut start = python(&engine, "session");
            start
                .arg(&config_path)
                .stdout(Stdio::from(log.try_clone().map_err(err)?))
                .stderr(Stdio::from(log))
                .stdin(Stdio::null());
            // Terminate the trusted host engine if the launcher dies, including SIGKILL/crash.
            let parent = unsafe { libc::getpid() };
            unsafe {
                start.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::getppid() != parent {
                        return Err(std::io::Error::other("Launcher exited during room startup"));
                    }
                    Ok(())
                });
            }
            let child = start.spawn().map_err(err)?;
            let mut session = Session {
                child,
                socket: ipc.join("control.sock"),
                engine: engine.clone(),
                fingerprint: fingerprint.clone(),
            };
            let deadline = Instant::now() + Duration::from_secs(60);
            while !session.socket.exists() {
                if let Some(status) = session.child.try_wait().map_err(err)? {
                    return Err(format!(
                        "Could not prepare a secure room ({status}): {}",
                        fs::read_to_string(root.join("room.log"))
                            .unwrap_or_default()
                            .chars()
                            .rev()
                            .take(6000)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect::<String>()
                    ));
                }
                if Instant::now() > deadline {
                    return Err("Room startup timed out; launch was stopped".into());
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            entry.insert(session);
        }
        let session = sessions.get(&game.id).ok_or("Room supervisor missing")?;
        if session.fingerprint != fingerprint {
            return Err("Room requirements changed while it was open. Close all room applications before trying again.".into());
        }
        let mut client = python(&engine, "exec");
        client
            .arg(&session.socket)
            .arg(serde_json::to_string(&command_request(&command)).map_err(err)?);
        Ok(client)
    }

    fn provision(
        engine: &Path,
        root: &Path,
        host_config: &mut Value,
        guest_config: &Path,
        game: &Game,
        command: &Command,
        packages: &[Value],
    ) -> Result<(), String> {
        let prefix = if game.runtime_kind == "proton" {
            Path::new(&game.prefix).join("pfx")
        } else {
            PathBuf::from(&game.prefix)
        };
        let runner_argv: Vec<_> =
            std::iter::once(command.get_program().to_string_lossy().into_owned()).collect();
        if !prefix.join("system.reg").is_file() {
            let mut bootstrap = runner_argv.clone();
            bootstrap.extend(["wineboot.exe".into(), "-u".into()]);
            let runner = which::which(&game.runtime_path)
                .map_err(err)?
                .canonicalize()
                .map_err(err)?;
            let wineserver = if game.runtime_kind == "proton" {
                runner
                    .parent()
                    .ok_or("Missing Proton directory")?
                    .join("files/bin/wineserver")
            } else {
                which::which("wineserver").map_err(err)?
            };
            fs::write(
                guest_config,
                serde_json::to_vec(
                    &json!({"display":host_config["display"],"screen":host_config["screen"],"once":true,"private_bus":game.runtime_kind=="proton","bootstrap":bootstrap,"wait_command":[wineserver,"-w"],"wine_prefix":prefix}),
                )
                .map_err(err)?,
            )
            .map_err(err)?;
            let path = root.join("bootstrap.json");
            fs::write(&path, serde_json::to_vec(host_config).map_err(err)?).map_err(err)?;
            let result = python(engine, "session").arg(path).output().map_err(err)?;
            if !result.status.success() {
                return Err(format!(
                    "Room prefix preparation failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                ));
            }
        }
        let mut compose = json!({"prefix":prefix,"packages":packages,"store":root.parent().and_then(Path::parent).ok_or("Invalid room data path")?.join("windows-runtimes")});
        let composed = output(python(engine, "compose").arg(compose.take().to_string()))?;
        let mounts = host_config["runtime_mounts"]
            .as_array_mut()
            .ok_or("Invalid room mount list")?;
        mounts.extend(
            composed["mounts"]
                .as_array()
                .ok_or("Invalid package mounts")?
                .iter()
                .cloned(),
        );
        let mut registry_commands = Vec::new();
        for registry in composed["registry"]
            .as_array()
            .ok_or("Invalid registry recipes")?
        {
            let mut argv = runner_argv.clone();
            argv.extend([
                "reg.exe".into(),
                "import".into(),
                registry.as_str().ok_or("Invalid registry path")?.into(),
            ]);
            registry_commands.push(argv);
        }
        fs::write(guest_config, serde_json::to_vec(&json!({"display":host_config["display"],"screen":host_config["screen"],"once":false,"private_bus":game.runtime_kind=="proton","registry_commands":registry_commands})).map_err(err)?).map_err(err)?;
        Ok(())
    }
}

impl Launcher {
    pub fn inspect_room(&self, game_id: i64) -> Result<Value, String> {
        self.rooms.inspect(game_id, false)
    }
    pub fn inspect_room_image(&self, game_id: i64) -> Result<Value, String> {
        self.rooms.inspect(game_id, true)
    }
    pub fn stop_room(&self, game_id: i64) -> Result<(), String> {
        let _guard = self.launch_lock.lock().map_err(err)?;
        self.rooms.stop(game_id)
    }
    pub fn install_windows_runtime(&self, source: String) -> Result<Value, String> {
        let _guard = self.launch_lock.lock().map_err(err)?;
        let engine = engine(&self.data)?;
        let source = PathBuf::from(source).canonicalize().map_err(err)?;
        let installed = output(
            python(&engine, "install")
                .arg(self.data.join("windows-runtimes"))
                .arg(&source),
        )?;
        self.connection()?.execute("INSERT INTO windows_runtime_sources(digest,source_path) VALUES (?1,?2) ON CONFLICT(digest) DO UPDATE SET source_path=excluded.source_path", params![installed["digest"].as_str().ok_or("Invalid runtime identity")?,source.to_string_lossy().as_ref()]).map_err(err)?;
        Ok(installed)
    }

    pub fn windows_runtimes(&self) -> Result<Value, String> {
        output(python(&engine(&self.data)?, "catalog").arg(self.data.join("windows-runtimes")))
    }

    pub fn configure_room(&self, game_id: i64, profile: RoomProfile) -> Result<(), String> {
        let _guard = self.launch_lock.lock().map_err(err)?;
        self.recover_rooms()?;
        valid_profile(&profile)?;
        let game = self
            .games()?
            .into_iter()
            .find(|game| game.id == game_id)
            .ok_or("Game not found")?;
        self.rooms.quiesce(game_id)?;
        recovery::ensure_idle(&game)?;
        let engine = engine(&self.data)?;
        // Resolve now so settings are reviewable and do not introduce a missing/broken runtime.
        let resolved = output(
            python(&engine, "resolve")
                .arg(self.data.join("windows-runtimes"))
                .arg(serde_json::to_string(&profile.requirements).map_err(err)?)
                .arg(serde_json::to_string(&profile.packages).map_err(err)?)
                .arg("mixed"),
        )?;
        let mut profile = profile;
        profile.packages = resolved["packages"]
            .as_array()
            .ok_or("Invalid runtime resolution")?
            .iter()
            .map(|p| {
                p["digest"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or("Invalid package pin".into())
            })
            .collect::<Result<Vec<_>, String>>()?;
        self.connection()?.execute("INSERT INTO room_profiles(game_id,profile_json) VALUES (?1,?2) ON CONFLICT(game_id) DO UPDATE SET profile_json=excluded.profile_json", params![game_id,serde_json::to_string(&profile).map_err(err)?]).map_err(err)?;
        Ok(())
    }

    pub(crate) fn room_command(&self, game: &Game, command: Command) -> Result<Command, String> {
        let profile = status(&self.connection()?, game.id).map_err(err)?.profile;
        valid_profile(&profile)?;
        self.exclusive_roots(game, &writable_roots(game, &self.data)?)?;
        self.rooms.command(&self.data, game, &profile, command)
    }

    pub(crate) fn prepare_room_runtimes(&self, game: &mut Game) -> Result<(), String> {
        let db = self.connection()?;
        let mut statement = db
            .prepare("SELECT digest,source_path FROM windows_runtime_sources")
            .map_err(err)?;
        let sources = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(err)?
            .collect::<Result<HashMap<_, _>, _>>()
            .map_err(err)?;
        let resolved = output(
            python(&engine(&self.data)?, "plan")
                .arg(self.data.join("windows-runtimes"))
                .arg(serde_json::to_string(&game.room.profile).map_err(err)?)
                .arg(
                    serde_json::to_string(
                        &std::iter::once(&game.executable)
                            .chain(game.tools.iter().map(|tool| &tool.executable))
                            .collect::<Vec<_>>(),
                    )
                    .map_err(err)?,
                )
                .arg(serde_json::to_string(&sources).map_err(err)?),
        )?;
        game.room.profile.packages = resolved["packages"]
            .as_array()
            .ok_or("Invalid runtime resolution")?
            .iter()
            .map(|p| {
                p["digest"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or("Invalid runtime identity".into())
            })
            .collect::<Result<Vec<_>, String>>()?;
        db.execute("INSERT INTO room_profiles(game_id,profile_json) VALUES (?1,?2) ON CONFLICT(game_id) DO UPDATE SET profile_json=excluded.profile_json", params![game.id,serde_json::to_string(&game.room.profile).map_err(err)?]).map_err(err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Launcher;

    #[test]
    fn room_screen_follows_the_game_setting_else_the_launcher_monitor() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::at(temp.path().join("state"), temp.path().join("logs")).unwrap();
        let exe = temp.path().join("Game.exe");
        fs::write(&exe, b"MZtest").unwrap();
        let mut game = launcher.import(&exe, None, "manual").unwrap();
        let screen = |game: &Game, monitor| xwayland_screen(game, monitor).ok();

        // No resolution of its own: fill the monitor the launcher reported, or refuse to guess.
        assert_eq!(
            screen(&game, Some((2560, 1440))),
            Some(json!({"fullscreen":true,"width":2560,"height":1440}))
        );
        assert_eq!(screen(&game, None), None);

        game.display_width = Some(1600);
        game.display_height = Some(900);
        for mode in ["fullscreen", "borderless"] {
            game.display_mode = mode.into();
            assert_eq!(
                screen(&game, Some((2560, 1440))),
                Some(json!({"fullscreen":true,"width":1600,"height":900}))
            );
        }
        game.display_mode = "windowed".into();
        assert_eq!(
            screen(&game, None),
            Some(json!({"fullscreen":false,"width":1600,"height":900}))
        );
    }
}
