// SPDX-License-Identifier: MIT
//! Room checkpoints. SQLite publishes complete manifests and journals filesystem switches.
//! Checkpoints are never hardlinks. Saves are copied from the current room into the staged
//! restore before any live directory is renamed. No live SQLite database is copied.
use crate::library::{Game, GameSettings, Launcher, ToolSettings};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Serialize)]
pub struct Checkpoint {
    pub id: i64,
    pub label: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecoveryStatus {
    pub supported: bool,
    pub checkpoints: Vec<Checkpoint>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Root {
    live: PathBuf,
    image: PathBuf,
    backend: String,
    saves: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    roots: Vec<Root>,
    settings: GameSettings,
    tools: Vec<ToolSettings>,
    #[serde(default)]
    room_profile: Option<crate::rooms::RoomProfile>,
}

#[derive(Serialize, Deserialize)]
struct Switch {
    live: PathBuf,
    stage: PathBuf,
    previous: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Journal {
    switches: Vec<Switch>,
}

fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn supported(executable: &Path) -> bool {
    save_folder(executable).is_some()
}

pub fn supported_game(game: &Game) -> bool {
    supported(Path::new(&game.executable))
        || !game.room.profile.save_paths.is_empty()
        || !game.room.profile.game_save_paths.is_empty()
        || !game.room.profile.home_save_paths.is_empty()
}

fn save_folder(executable: &Path) -> Option<&'static str> {
    match executable
        .file_name()?
        .to_str()?
        .to_ascii_lowercase()
        .as_str()
    {
        "skyrimse.exe" => Some("Skyrim Special Edition"),
        "fallout4.exe" => Some("Fallout4"),
        _ => None,
    }
}

pub fn status(
    db: &Connection,
    game_id: i64,
    executable: &Path,
) -> rusqlite::Result<RecoveryStatus> {
    let mut statement = db.prepare("SELECT id,label,created_at FROM room_checkpoints WHERE game_id=?1 AND state='ready' ORDER BY id DESC")?;
    let checkpoints = statement
        .query_map([game_id], |row| {
            Ok(Checkpoint {
                id: row.get(0)?,
                label: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(RecoveryStatus {
        supported: supported(executable) || {
            let profile = crate::rooms::status(db, game_id)?.profile;
            !profile.save_paths.is_empty()
                || !profile.game_save_paths.is_empty()
                || !profile.home_save_paths.is_empty()
        },
        checkpoints,
    })
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

pub(crate) fn prepare_prefix(prefix: &Path) -> Result<(), String> {
    if exists(prefix) {
        if !prefix.is_dir() {
            return Err("The room prefix is not a directory".into());
        }
        return Ok(());
    }
    fs::create_dir_all(prefix.parent().ok_or("Missing prefix parent")?).map_err(error)?;
    if removable_subvolumes(prefix.parent().ok_or("Missing prefix parent")?)
        && Command::new("btrfs")
            .args(["subvolume", "create"])
            .arg(prefix)
            .output()
            .is_ok_and(|r| r.status.success())
    {
        sync_directory(prefix.parent().ok_or("Missing prefix parent")?)?;
        return Ok(());
    }
    fs::create_dir_all(prefix).map_err(error)
}

fn removable_subvolumes(path: &Path) -> bool {
    let privileged = unsafe { libc::geteuid() == 0 };
    privileged || {
        Command::new("findmnt")
            .args(["-n", "-o", "OPTIONS", "--target"])
            .arg(path)
            .output()
            .is_ok_and(|result| {
                result.status.success()
                    && String::from_utf8_lossy(&result.stdout)
                        .trim()
                        .split(',')
                        .any(|option| option == "user_subvol_rm_allowed")
            })
    }
}

fn has_nested_subvolumes(path: &Path) -> Result<bool, String> {
    for entry in fs::read_dir(path).map_err(error)? {
        let entry = entry.map_err(error)?;
        if entry.file_type().map_err(error)?.is_dir()
            && (entry.metadata().map_err(error)?.ino() == 256
                || has_nested_subvolumes(&entry.path())?)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(error)
}

fn run(command: &mut Command) -> Result<(), String> {
    let result = command.output().map_err(error)?;
    if !result.status.success() {
        return Err(format!(
            "Room storage operation failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    Ok(())
}

/// Only an actual Btrfs subvolume can be snapshotted. Ordinary directories, other filesystems
/// and cross-filesystem destinations use GNU cp's reflink support, with independent copies
/// as the fallback. cp -a preserves symlinks without following them.
fn clone_tree(source: &Path, destination: &Path, read_only: bool) -> Result<String, String> {
    if exists(destination) {
        return Err("Checkpoint staging path already exists".into());
    }
    // The B-tree-search based `btrfs subvolume show/list` requires admin rights on
    // common mounts. Directory inode 256 and a no-follow directory walk work rootlessly.
    if fs::metadata(source).map_err(error)?.ino() == 256
        && removable_subvolumes(destination.parent().ok_or("Missing snapshot parent")?)
        && !has_nested_subvolumes(source)?
    {
        let mut command = Command::new("btrfs");
        command.args(["subvolume", "snapshot"]);
        if read_only {
            command.arg("-r");
        }
        if command
            .arg(source)
            .arg(destination)
            .output()
            .is_ok_and(|r| r.status.success())
        {
            run(Command::new("sync").arg("-f").arg(destination))?;
            sync_directory(destination.parent().ok_or("Missing storage parent")?)?;
            return Ok("btrfs".into());
        }
        if exists(destination) {
            return Err("Incomplete Btrfs snapshot; source was left unchanged".into());
        }
    }
    run(Command::new("cp")
        .args(["--archive", "--reflink=auto", "--"])
        .arg(source)
        .arg(destination))?;
    run(Command::new("sync").arg("-f").arg(destination))?;
    sync_directory(destination.parent().ok_or("Missing storage parent")?)?;
    Ok("reflink-or-copy".into())
}

/// Reject ancestors with symlinks: a save overlay must never accidentally write through a
/// staged Wine Documents link into the live prefix or host Documents directory.
fn no_link_ancestors(root: &Path, relative: &Path) -> Result<(), String> {
    let mut path = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err("Invalid save mapping".into());
        };
        path.push(name);
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "Save path uses a link ({}) and cannot be safely restored yet",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

/// Wine often links Documents to host Documents. Those saves already live outside the
/// captured trees. Leave the matching link alone; never traverse it to transplant files.
fn external_save(
    root: &Path,
    stage: Option<&Path>,
    relative: &Path,
    managed: &[PathBuf],
) -> Result<bool, String> {
    let mut path = root.to_path_buf();
    let mut walked = PathBuf::new();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err("Invalid save mapping".into());
        };
        path.push(name);
        walked.push(name);
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            let resolved = path.canonicalize().map_err(error)?;
            if managed.iter().any(|tree| resolved.starts_with(tree)) {
                return Err("Save path links into another captured directory; recovery needs a dedicated save mapping".into());
            }
            if let Some(stage) = stage {
                let staged_link = stage.join(&walked);
                if fs::read_link(&path).map_err(error)?
                    != fs::read_link(&staged_link).map_err(error)?
                {
                    return Err("The save directory link changed since this checkpoint; recovery was stopped to protect saves".into());
                }
            }
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_save_contents(path: &Path, managed: &[PathBuf]) -> Result<(), String> {
    if !exists(path) {
        return Ok(());
    }
    for entry in fs::read_dir(path).map_err(error)? {
        let entry = entry.map_err(error)?;
        let kind = entry.file_type().map_err(error)?;
        if kind.is_symlink() {
            let resolved = entry.path().canonicalize().map_err(error)?;
            if managed.iter().any(|root| resolved.starts_with(root)) {
                return Err(
                    "A save file links into a captured tree; recovery was stopped to protect saves"
                        .into(),
                );
            }
        } else if kind.is_dir() {
            validate_save_contents(&entry.path(), managed)?;
        } else if !kind.is_file() {
            return Err(
                "Save directory contains a special file that cannot be preserved safely".into(),
            );
        }
    }
    Ok(())
}

fn save_mappings(root: &Path, game: &Game) -> Result<Vec<PathBuf>, String> {
    let mut saves = Vec::new();
    let data = Path::new(&game.prefix)
        .parent()
        .and_then(Path::parent)
        .ok_or("Invalid managed prefix path")?;
    let home = data
        .join("rooms")
        .join(format!("game-{}", game.id))
        .join("home");
    if home.canonicalize().ok().as_deref() == Some(root) {
        saves.extend(game.room.profile.home_save_paths.iter().map(PathBuf::from));
        if let Some(folder) = save_folder(Path::new(&game.executable)) {
            saves.push(
                PathBuf::from("Documents/My Games")
                    .join(folder)
                    .join("Saves"),
            );
        }
    }
    let is_prefix = root == Path::new(&game.prefix).canonicalize().map_err(error)?;
    if is_prefix {
        for path in &game.room.profile.save_paths {
            saves.push(if game.runtime_kind == "proton" {
                PathBuf::from("pfx").join(path)
            } else {
                PathBuf::from(path)
            });
        }
    }
    if let Ok(directory) = Path::new(&game.executable)
        .parent()
        .ok_or("Missing game directory")?
        .canonicalize()
    {
        if let Ok(relative) = directory.strip_prefix(root) {
            for path in &game.room.profile.game_save_paths {
                saves.push(relative.join(path));
            }
        }
    }
    if is_prefix && save_folder(Path::new(&game.executable)).is_some() {
        let base = if game.runtime_kind == "proton" {
            PathBuf::from("pfx/drive_c/users")
        } else {
            PathBuf::from("drive_c/users")
        };
        if let Ok(users) = fs::read_dir(root.join(&base)) {
            for user in users {
                let user = user.map_err(error)?;
                if !user.file_type().map_err(error)?.is_dir() {
                    continue;
                }
                let path = base
                    .join(user.file_name())
                    .join("Documents/My Games")
                    .join(save_folder(Path::new(&game.executable)).ok_or("No save mapping")?)
                    .join("Saves");
                // Include absent save directories too: the first session may create them.
                saves.push(path);
            }
        }
        // Proton creates steamuser only on the first boot.
        if game.runtime_kind == "proton" {
            saves.push(
                base.join("steamuser/Documents/My Games")
                    .join(save_folder(Path::new(&game.executable)).ok_or("No save mapping")?)
                    .join("Saves"),
            );
        }
    }
    // MO2 portable profiles can be in the game folder, manager folder or working folder.
    for tool in &game.tools {
        if !Path::new(&tool.executable)
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("ModOrganizer.exe"))
        {
            continue;
        }
        for directory in [
            Path::new(&tool.executable).parent(),
            Some(Path::new(&tool.working_directory)),
        ]
        .into_iter()
        .flatten()
        {
            let directory = directory.canonicalize().map_err(error)?;
            if let Ok(relative) = directory.strip_prefix(root) {
                let profiles = relative.join("profiles");
                if exists(&root.join(&profiles)) {
                    no_link_ancestors(root, &profiles)?;
                    for profile in fs::read_dir(root.join(&profiles)).map_err(error)? {
                        let profile = profile.map_err(error)?;
                        if profile.file_type().map_err(error)?.is_dir() {
                            saves.push(profiles.join(profile.file_name()).join("saves"));
                        }
                    }
                }
            }
        }
    }
    saves.sort();
    saves.dedup();
    Ok(saves)
}

fn save_locations(
    game: &Game,
    roots: &[PathBuf],
    saved: &[Root],
) -> Result<std::collections::HashMap<PathBuf, Vec<PathBuf>>, String> {
    let mut mappings: std::collections::HashMap<PathBuf, Vec<PathBuf>> =
        std::collections::HashMap::new();
    let mut candidates = Vec::new();
    for root in roots {
        for save in save_mappings(root, game)? {
            candidates.push((root.clone(), save));
        }
    }
    for root in saved {
        for save in &root.saves {
            candidates.push((root.live.clone(), save.clone()));
        }
    }
    for (source, relative) in candidates {
        // Resolve the existing part of the path, including Wine's Documents/My Documents
        // aliases. Missing save directories are retained for a game's first session.
        let original = source.join(&relative);
        let mut ancestor = original.as_path();
        let mut tail = Vec::new();
        while !exists(ancestor) {
            tail.push(
                ancestor
                    .file_name()
                    .ok_or("Invalid save path")?
                    .to_os_string(),
            );
            ancestor = ancestor.parent().ok_or("Invalid save path")?;
        }
        let mut resolved = ancestor.canonicalize().map_err(error)?;
        for part in tail.into_iter().rev() {
            resolved.push(part);
        }
        if let Some(root) = roots.iter().find(|root| resolved.starts_with(root)) {
            if &resolved == root {
                return Err(
                    "Save alias covers an entire managed directory; recovery was stopped".into(),
                );
            }
            mappings
                .entry(root.clone())
                .or_default()
                .push(resolved.strip_prefix(root).map_err(error)?.to_path_buf());
        } else {
            // Keep external aliases as aliases so restore can check that their link target
            // has not changed; it never writes through them into host Documents.
            mappings.entry(source).or_default().push(relative);
        }
    }
    for saves in mappings.values_mut() {
        saves.sort();
        saves.dedup();
    }
    Ok(mappings)
}

fn roots(game: &Game, data: &Path) -> Result<Vec<PathBuf>, String> {
    let prefix = Path::new(&game.prefix).canonicalize().map_err(error)?;
    let home = data
        .join("rooms")
        .join(format!("game-{}", game.id))
        .join("home");
    fs::create_dir_all(&home).map_err(error)?;
    let home = home.canonicalize().map_err(error)?;
    let mut candidates = vec![
        prefix.clone(),
        home.clone(),
        Path::new(&game.executable)
            .parent()
            .ok_or("Missing game directory")?
            .canonicalize()
            .map_err(error)?,
    ];
    for tool in &game.tools {
        candidates.push(
            Path::new(&tool.executable)
                .parent()
                .ok_or("Missing companion directory")?
                .canonicalize()
                .map_err(error)?,
        );
        candidates.push(
            Path::new(&tool.working_directory)
                .canonicalize()
                .map_err(error)?,
        );
    }
    let data = data.canonicalize().map_err(error)?;
    for path in &candidates {
        if !path.is_dir()
            || path.parent().is_none()
            || path.components().count() < 3
            || (path != &prefix
                && path != &home
                && !path.starts_with(&prefix)
                && (data.starts_with(path) || path.starts_with(&data)))
            || [
                "/usr", "/etc", "/var", "/bin", "/sbin", "/lib", "/lib64", "/boot", "/proc",
                "/sys", "/dev", "/run",
            ]
            .iter()
            .any(|reserved| path.starts_with(reserved))
            || dirs::home_dir().is_some_and(|home| home.starts_with(path))
        {
            return Err(format!("{} is too broad or shared to checkpoint safely. Put the game and companions in dedicated folders.", path.display()));
        }
    }
    candidates.sort_by_key(|path| path.components().count());
    let mut result: Vec<PathBuf> = Vec::new();
    for path in candidates {
        if !result.iter().any(|root| path.starts_with(root)) {
            result.push(path);
        }
    }
    Ok(result)
}

/// A launcher process may have exited while Wine children remain alive. Check inherited prefix
/// environment as well as launch records before allowing a snapshot or directory switch.
pub(crate) fn ensure_idle(game: &Game) -> Result<(), String> {
    if game
        .launch
        .as_ref()
        .is_some_and(|l| ["starting", "running"].contains(&l.state.as_str()))
        || game.tools.iter().any(|t| {
            t.launch
                .as_ref()
                .is_some_and(|l| ["starting", "running"].contains(&l.state.as_str()))
        })
    {
        return Err(
            "Close the game and all companion apps before restoring or checkpointing the room"
                .into(),
        );
    }
    let uid = fs::metadata("/proc/self").map_err(error)?.uid();
    for process in fs::read_dir("/proc").map_err(error)? {
        let process = process.map_err(error)?;
        if process
            .file_name()
            .to_string_lossy()
            .parse::<u32>()
            .is_err()
        {
            continue;
        }
        if !process
            .metadata()
            .is_ok_and(|metadata| metadata.uid() == uid)
        {
            continue;
        }
        let environment = match fs::read(process.path().join("environ")) {
            Ok(environment) => environment,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            // Kernel threads and non-dumpable host processes can have unreadable environments.
            Err(_) => continue,
        };
        let matching = environment.split(|byte| *byte == 0).any(|entry| {
            [
                b"WINEPREFIX=".as_slice(),
                b"STEAM_COMPAT_DATA_PATH=".as_slice(),
            ]
            .iter()
            .any(|key| {
                entry.strip_prefix(*key).is_some_and(|value| {
                    let value = Path::new(std::ffi::OsStr::from_bytes(value));
                    value == Path::new(&game.prefix)
                        || value == Path::new(&game.prefix).join("pfx")
                        || value
                            .canonicalize()
                            .ok()
                            .zip(Path::new(&game.prefix).canonicalize().ok())
                            .is_some_and(|(a, b)| a == b)
                })
            })
        });
        if matching {
            return Err(
                "A Wine process is still using this room. Close it before recovery.".into(),
            );
        }
    }
    Ok(())
}

use std::os::unix::ffi::OsStrExt;

impl Launcher {
    fn recovery_game(&self, game_id: i64) -> Result<Game, String> {
        self.games()?
            .into_iter()
            .find(|game| game.id == game_id)
            .ok_or("Game not found".into())
    }

    pub(crate) fn cleanup_room_storage(&self) -> Result<(), String> {
        let db = self.connection()?;
        let mut query = db
            .prepare("SELECT path FROM room_storage_garbage")
            .map_err(error)?;
        let paths = query
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        for path in paths {
            let path = PathBuf::from(path);
            let removed = if !exists(&path) {
                true
            } else if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                fs::remove_file(&path).is_ok()
            } else if Command::new("btrfs")
                .args(["subvolume", "delete"])
                .arg(&path)
                .output()
                .is_ok_and(|r| r.status.success())
            {
                true
            } else {
                fs::remove_dir_all(&path).is_ok()
            };
            if removed {
                if let Some(parent) = path.parent() {
                    if parent.exists() {
                        sync_directory(parent)?;
                    }
                    if parent.starts_with(self.data.join("checkpoints")) {
                        let _ = fs::remove_dir(parent);
                    }
                }
                db.execute(
                    "DELETE FROM room_storage_garbage WHERE path=?1",
                    [path.to_string_lossy().as_ref()],
                )
                .map_err(error)?;
            }
        }
        Ok(())
    }

    pub(crate) fn exclusive_roots(&self, game: &Game, paths: &[PathBuf]) -> Result<(), String> {
        let db = self.connection()?;
        let mut query = db.prepare("SELECT executable,1 FROM games WHERE id<>?1 UNION ALL SELECT executable,1 FROM tools WHERE game_id<>?1 AND recovery_archived=0 UNION ALL SELECT working_directory,0 FROM tools WHERE game_id<>?1 AND recovery_archived=0 UNION ALL SELECT path,0 FROM prefixes WHERE game_id<>?1").map_err(error)?;
        let others = query
            .query_map([game.id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
            })
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        for (path, executable) in others {
            if path.is_empty() {
                continue;
            }
            let path = PathBuf::from(path);
            let path = if executable {
                path.parent().ok_or("Invalid companion path")?.to_path_buf()
            } else {
                path
            };
            let path = path.canonicalize().unwrap_or(path);
            if paths
                .iter()
                .any(|root| path.starts_with(root) || root.starts_with(&path))
            {
                return Err("This room shares a game or companion folder with another room. Recovery requires dedicated folders so other games cannot be changed.".into());
            }
        }
        Ok(())
    }

    fn prune_session_checkpoints(&self, game_id: i64) -> Result<(), String> {
        let mut db = self.connection()?;
        let mut query = db.prepare("SELECT id,manifest_json FROM room_checkpoints WHERE game_id=?1 AND label='Before session' AND state='ready' AND id NOT IN (SELECT checkpoint_id FROM room_restores) ORDER BY id DESC LIMIT -1 OFFSET 3").map_err(error)?;
        let old = query
            .query_map([game_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        drop(query);
        let tx = db.transaction().map_err(error)?;
        for (id, json) in old {
            let manifest: Manifest = serde_json::from_str(&json).map_err(error)?;
            for root in manifest.roots {
                tx.execute(
                    "INSERT OR IGNORE INTO room_storage_garbage(path) VALUES (?1)",
                    [root.image.to_string_lossy().as_ref()],
                )
                .map_err(error)?;
            }
            tx.execute("DELETE FROM room_checkpoints WHERE id=?1", [id])
                .map_err(error)?;
        }
        tx.commit().map_err(error)?;
        self.cleanup_room_storage()
    }

    pub fn create_checkpoint(&self, game_id: i64) -> Result<i64, String> {
        let _guard = self.launch_lock.lock().map_err(error)?;
        self.recover_rooms()?;
        self.rooms.quiesce(game_id)?;
        let game = self.recovery_game(game_id)?;
        self.checkpoint_room(&game, "Manual checkpoint")
    }

    pub(crate) fn checkpoint_room(&self, game: &Game, label: &str) -> Result<i64, String> {
        if !supported_game(game) {
            return Err("Save-preserving recovery is not available for this game yet".into());
        }
        ensure_idle(game)?;
        prepare_prefix(Path::new(&game.prefix))?;
        let paths = roots(game, &self.data)?;
        self.exclusive_roots(game, &paths)?;
        let mut save_locations = save_locations(game, &paths, &[])?;
        for path in &paths {
            for save in save_locations.get(path).into_iter().flatten() {
                if !external_save(path, None, save, &paths)? {
                    no_link_ancestors(path, save)?;
                    validate_save_contents(&path.join(save), &paths)?;
                }
            }
        }
        self.cleanup_room_storage()?;
        let mut db = self.connection()?;
        db.execute(
            "INSERT INTO room_checkpoints(game_id,label,state) VALUES (?1,?2,'preparing')",
            params![game.id, label],
        )
        .map_err(error)?;
        let id = db.last_insert_rowid();
        let directory = self
            .data
            .join("checkpoints")
            .join(format!("game-{}", game.id))
            .join(id.to_string());
        fs::create_dir_all(&directory).map_err(error)?;
        fs::set_permissions(
            self.data.join("checkpoints"),
            fs::Permissions::from_mode(0o700),
        )
        .map_err(error)?;
        let mut captured = Vec::new();
        for (index, path) in paths.into_iter().enumerate() {
            let saves = save_locations.remove(&path).unwrap_or_default();
            let image = directory.join(format!("root-{index}"));
            db.execute(
                "INSERT INTO room_storage_garbage(path) VALUES (?1)",
                [image.to_string_lossy().as_ref()],
            )
            .map_err(error)?;
            let backend = clone_tree(&path, &image, true)?;
            captured.push(Root {
                live: path,
                image,
                backend,
                saves,
            });
        }
        let manifest = Manifest {
            version: 1,
            roots: captured,
            room_profile: Some(game.room.profile.clone()),
            settings: GameSettings {
                id: game.id,
                name: game.name.clone(),
                runtime_kind: game.runtime_kind.clone(),
                runtime_path: game.runtime_path.clone(),
                arguments: game.arguments.clone(),
                display_mode: game.display_mode.clone(),
                display_width: game.display_width,
                display_height: game.display_height,
                launch_via_tool_id: game.launch_via_tool_id,
            },
            tools: game
                .tools
                .iter()
                .map(|tool| ToolSettings {
                    id: Some(tool.id),
                    game_id: game.id,
                    kind: tool.kind.clone(),
                    name: tool.name.clone(),
                    executable: tool.executable.clone(),
                    arguments: tool.arguments.clone(),
                    working_directory: tool.working_directory.clone(),
                })
                .collect(),
        };
        let tx = db.transaction().map_err(error)?;
        tx.execute(
            "UPDATE room_checkpoints SET state='ready',manifest_json=?1 WHERE id=?2",
            params![serde_json::to_string(&manifest).map_err(error)?, id],
        )
        .map_err(error)?;
        for root in &manifest.roots {
            tx.execute(
                "DELETE FROM room_storage_garbage WHERE path=?1",
                [root.image.to_string_lossy().as_ref()],
            )
            .map_err(error)?;
        }
        tx.commit().map_err(error)?;
        self.prune_session_checkpoints(game.id)?;
        Ok(id)
    }

    pub fn restore_checkpoint(&self, game_id: i64, checkpoint_id: i64) -> Result<(), String> {
        let _guard = self.launch_lock.lock().map_err(error)?;
        self.recover_rooms()?;
        self.rooms.quiesce(game_id)?;
        let game = self.recovery_game(game_id)?;
        ensure_idle(&game)?;
        let mut db = self.connection()?;
        let json: String = db.query_row("SELECT manifest_json FROM room_checkpoints WHERE id=?1 AND game_id=?2 AND state='ready'", params![checkpoint_id, game_id], |row| row.get(0)).map_err(|_| "Checkpoint not found for this room")?;
        let manifest: Manifest = serde_json::from_str(&json).map_err(error)?;
        if manifest.version != 1 {
            return Err("Unsupported checkpoint version".into());
        }
        let current_roots = roots(&game, &self.data)?;
        let mut saved_roots: Vec<_> = manifest
            .roots
            .iter()
            .map(|root| root.live.clone())
            .collect();
        let mut expected_roots = current_roots.clone();
        saved_roots.sort();
        expected_roots.sort();
        if saved_roots != expected_roots || manifest.settings.id != game_id {
            return Err("Room folders changed since this checkpoint. Restore their original locations before recovery.".into());
        }
        for (index, root) in manifest.roots.iter().enumerate() {
            let expected = self
                .data
                .join("checkpoints")
                .join(format!("game-{game_id}"))
                .join(checkpoint_id.to_string())
                .join(format!("root-{index}"));
            if root.image != expected {
                return Err("Invalid checkpoint storage path".into());
            }
        }
        if manifest.settings.runtime_kind != game.runtime_kind {
            return Err("Switch back to the checkpoint's Wine/Proton type before restoring so saves can be mapped safely".into());
        }
        // Preserve today's state first, making Restore itself reversible.
        let undo = self.checkpoint_room(&game, "Before restore — undo")?;
        let mut save_locations = save_locations(&game, &current_roots, &manifest.roots)?;
        let mut switches = Vec::new();
        for (index, root) in manifest.roots.iter().enumerate() {
            let parent = root.live.parent().ok_or("Invalid checkpoint path")?;
            let stage = parent.join(format!(".vessel-stage-{game_id}-{undo}-{index}"));
            let previous = parent.join(format!(".vessel-previous-{game_id}-{undo}-{index}"));
            if exists(&previous) {
                return Err("Restore backup path already exists".into());
            }
            db.execute(
                "INSERT INTO room_storage_garbage(path) VALUES (?1)",
                [stage.to_string_lossy().as_ref()],
            )
            .map_err(error)?;
            clone_tree(&root.image, &stage, false)?;
            // Re-discover saves as profiles/users may have been created after this checkpoint.
            let saves = save_locations.remove(&root.live).unwrap_or_default();
            for save in saves {
                if external_save(&root.live, Some(&stage), &save, &current_roots)? {
                    continue;
                }
                no_link_ancestors(&root.live, &save)?;
                no_link_ancestors(&stage, &save)?;
                let current = root.live.join(&save);
                let destination = stage.join(&save);
                validate_save_contents(&current, &current_roots)?;
                // No current saves means preserve the absence too, rather than resurrecting old saves.
                if exists(&destination) {
                    if !destination.is_dir() {
                        return Err("Save mapping is not a directory".into());
                    }
                    fs::remove_dir_all(&destination).map_err(error)?;
                }
                if exists(&current) {
                    fs::create_dir_all(destination.parent().ok_or("Invalid save path")?)
                        .map_err(error)?;
                    clone_tree(&current, &destination, false)?;
                }
            }
            run(Command::new("sync").arg("-f").arg(&stage))?;
            sync_directory(parent)?;
            switches.push(Switch {
                live: root.live.clone(),
                stage,
                previous,
            });
        }
        let tx = db.transaction().map_err(error)?;
        for switch in &switches {
            tx.execute(
                "DELETE FROM room_storage_garbage WHERE path=?1",
                [switch.stage.to_string_lossy().as_ref()],
            )
            .map_err(error)?;
        }
        tx.execute(
            "INSERT INTO room_restores(game_id,checkpoint_id,journal_json) VALUES (?1,?2,?3)",
            params![
                game_id,
                checkpoint_id,
                serde_json::to_string(&Journal { switches }).map_err(error)?
            ],
        )
        .map_err(error)?;
        tx.commit().map_err(error)?;
        self.finish_restore(game_id)
    }

    pub(crate) fn recover_rooms(&self) -> Result<(), String> {
        let db = self.connection()?;
        let mut query = db
            .prepare("SELECT game_id FROM room_restores ORDER BY game_id")
            .map_err(error)?;
        let ids = query
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        for id in ids {
            let game = self.recovery_game(id)?;
            ensure_idle(&game)?;
            self.finish_restore(id)?;
        }
        Ok(())
    }

    fn finish_restore(&self, game_id: i64) -> Result<(), String> {
        let mut db = self.connection()?;
        let (journal, manifest): (String, String) = db.query_row("SELECT r.journal_json,c.manifest_json FROM room_restores r JOIN room_checkpoints c ON c.id=r.checkpoint_id WHERE r.game_id=?1", [game_id], |row| Ok((row.get(0)?, row.get(1)?))).map_err(error)?;
        let journal: Journal = serde_json::from_str(&journal).map_err(error)?;
        let manifest: Manifest = serde_json::from_str(&manifest).map_err(error)?;
        if journal.switches.len() != manifest.roots.len() || manifest.settings.id != game_id {
            return Err("Invalid restore journal; no directories were changed".into());
        }
        for (switch, root) in journal.switches.iter().zip(&manifest.roots) {
            let valid_name = |path: &Path, kind: &str| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&format!(".vessel-{kind}-{game_id}-")))
            };
            if switch.live != root.live
                || !root.live.is_absolute()
                || switch.stage.parent() != root.live.parent()
                || switch.previous.parent() != root.live.parent()
                || !valid_name(&switch.stage, "stage")
                || !valid_name(&switch.previous, "previous")
            {
                return Err("Invalid restore journal paths; no directories were changed".into());
            }
        }
        for switch in &journal.switches {
            if exists(&switch.stage) {
                if exists(&switch.live) {
                    if exists(&switch.previous) {
                        return Err(
                            "Room changed during recovery; retained all versions for safety".into(),
                        );
                    }
                    fs::rename(&switch.live, &switch.previous).map_err(error)?;
                    sync_directory(switch.live.parent().ok_or("Invalid room path")?)?;
                }
                fs::rename(&switch.stage, &switch.live).map_err(error)?;
                sync_directory(switch.live.parent().ok_or("Invalid room path")?)?;
            } else if !exists(&switch.live) {
                return Err("Interrupted restore is missing its staged directory; retained the previous version".into());
            }
        }
        let tx = db.transaction().map_err(error)?;
        for switch in &journal.switches {
            tx.execute(
                "INSERT OR IGNORE INTO room_storage_garbage(path) VALUES (?1)",
                [switch.previous.to_string_lossy().as_ref()],
            )
            .map_err(error)?;
        }
        // Archive rather than delete: launches has an ON DELETE CASCADE foreign key.
        // Preserve historical labels/kinds even when this restore changes a profile.
        tx.execute("UPDATE launches SET companion_kind=COALESCE(companion_kind,(SELECT kind FROM tools WHERE id=launches.tool_id)),companion_name=COALESCE(companion_name,(SELECT name FROM tools WHERE id=launches.tool_id)) WHERE game_id=?1 AND tool_id IS NOT NULL", [game_id]).map_err(error)?;
        tx.execute(
            "UPDATE tools SET recovery_archived=1 WHERE game_id=?1",
            [game_id],
        )
        .map_err(error)?;
        let mut tool_ids = HashMap::new();
        for tool in &manifest.tools {
            let updated = tx.execute("UPDATE tools SET name=?1,executable=?2,kind=?3,arguments_json=?4,working_directory=?5,recovery_archived=0 WHERE id=?6 AND game_id=?7", params![tool.name,tool.executable,tool.kind,serde_json::to_string(&tool.arguments).map_err(error)?,tool.working_directory,tool.id,game_id]).map_err(error)?;
            let restored_id = if updated == 1 {
                tool.id.ok_or("Missing tool ID")?
            } else {
                tx.execute("INSERT INTO tools(game_id,name,executable,kind,arguments_json,working_directory) VALUES (?1,?2,?3,?4,?5,?6)", params![game_id, tool.name, tool.executable, tool.kind, serde_json::to_string(&tool.arguments).map_err(error)?, tool.working_directory]).map_err(error)?;
                tx.last_insert_rowid()
            };
            if let Some(old) = tool.id {
                tool_ids.insert(old, restored_id);
            }
        }
        let settings = &manifest.settings;
        if let Some(profile) = &manifest.room_profile {
            tx.execute("INSERT INTO room_profiles(game_id,profile_json) VALUES (?1,?2) ON CONFLICT(game_id) DO UPDATE SET profile_json=excluded.profile_json", params![game_id,serde_json::to_string(profile).map_err(error)?]).map_err(error)?;
        }
        let via = settings
            .launch_via_tool_id
            .and_then(|id| tool_ids.get(&id))
            .copied();
        tx.execute("UPDATE games SET name=?1,runtime_kind=?2,runtime_path=?3,arguments_json=?4,display_mode=?5,display_width=?6,display_height=?7,launch_via_tool_id=?8 WHERE id=?9", params![settings.name,settings.runtime_kind,settings.runtime_path,serde_json::to_string(&settings.arguments).map_err(error)?,settings.display_mode,settings.display_width,settings.display_height,via,game_id]).map_err(error)?;
        tx.execute("DELETE FROM room_restores WHERE game_id=?1", [game_id])
            .map_err(error)?;
        tx.commit().map_err(error)?;
        // Cleanup follows commit; the independent undo checkpoint remains published.
        self.cleanup_room_storage()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn fixture() -> (tempfile::TempDir, Launcher, Game) {
        let temp = tempfile::tempdir().unwrap();
        let game_dir = temp.path().join("games/Skyrim Special Edition");
        fs::create_dir_all(&game_dir).unwrap();
        let exe = game_dir.join("SkyrimSE.exe");
        fs::write(&exe, b"MZ game").unwrap();
        fs::write(game_dir.join("mod.dll"), b"working mod").unwrap();
        let service = Launcher::at(temp.path().join("state"), temp.path().join("logs")).unwrap();
        let game = service.import(&exe, None, "manual").unwrap();
        service
            .connection()
            .unwrap()
            .execute(
                "UPDATE games SET runtime_kind='wine',runtime_path='/bin/true' WHERE id=?1",
                [game.id],
            )
            .unwrap();
        let prefix = Path::new(&game.prefix);
        fs::create_dir_all(prefix).unwrap();
        fs::write(prefix.join("system.reg"), b"working registry").unwrap();
        let saves =
            prefix.join("drive_c/users/player/Documents/My Games/Skyrim Special Edition/Saves");
        fs::create_dir_all(&saves).unwrap();
        fs::write(saves.join("slot.ess"), b"old progress").unwrap();
        let game = service.recovery_game(game.id).unwrap();
        (temp, service, game)
    }

    fn saves(game: &Game) -> PathBuf {
        Path::new(&game.prefix)
            .join("drive_c/users/player/Documents/My Games/Skyrim Special Edition/Saves")
    }

    #[test]
    fn restore_reverts_files_and_registry_preserves_new_saves_and_can_be_undone() {
        let (_temp, service, game) = fixture();
        let id = service.create_checkpoint(game.id).unwrap();
        let game_dir = Path::new(&game.executable).parent().unwrap();
        fs::write(game_dir.join("mod.dll"), b"broken mod").unwrap();
        fs::write(game_dir.join("new.dll"), b"new mod").unwrap();
        fs::write(
            Path::new(&game.prefix).join("system.reg"),
            b"broken registry",
        )
        .unwrap();
        fs::write(saves(&game).join("slot.ess"), b"new progress").unwrap();
        fs::write(saves(&game).join("new.ess"), b"new slot").unwrap();
        service
            .connection()
            .unwrap()
            .execute(
                "INSERT INTO launches(game_id,state,exit_code) VALUES (?1,'finished',0)",
                [game.id],
            )
            .unwrap();
        service.restore_checkpoint(game.id, id).unwrap();
        assert_eq!(fs::read(game_dir.join("mod.dll")).unwrap(), b"working mod");
        assert!(!game_dir.join("new.dll").exists());
        assert_eq!(
            fs::read(Path::new(&game.prefix).join("system.reg")).unwrap(),
            b"working registry"
        );
        assert_eq!(
            fs::read(saves(&game).join("slot.ess")).unwrap(),
            b"new progress"
        );
        assert_eq!(fs::read(saves(&game).join("new.ess")).unwrap(), b"new slot");
        let restored = service.recovery_game(game.id).unwrap();
        assert_eq!(restored.sessions.len(), 1);
        let undo = restored
            .recovery
            .checkpoints
            .iter()
            .find(|c| c.label == "Before restore — undo")
            .unwrap()
            .id;
        service.restore_checkpoint(game.id, undo).unwrap();
        assert_eq!(fs::read(game_dir.join("mod.dll")).unwrap(), b"broken mod");
        assert!(game_dir.join("new.dll").exists());
        assert_eq!(
            fs::read(saves(&game).join("slot.ess")).unwrap(),
            b"new progress"
        );
    }

    #[test]
    fn checkpoints_are_independent_of_in_place_writes_and_do_not_follow_symlinks() {
        let (temp, service, game) = fixture();
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, b"host file").unwrap();
        symlink(&outside, Path::new(&game.prefix).join("host-link")).unwrap();
        let id = service.create_checkpoint(game.id).unwrap();
        let image = service
            .data
            .join(format!("checkpoints/game-{}/{id}/root-0", game.id));
        let json: String = service
            .connection()
            .unwrap()
            .query_row(
                "SELECT manifest_json FROM room_checkpoints WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        let manifest: Manifest = serde_json::from_str(&json).unwrap();
        let root = manifest
            .roots
            .iter()
            .find(|r| r.live == Path::new(&game.prefix))
            .unwrap();
        if root.backend != "btrfs" {
            assert_ne!(
                fs::metadata(Path::new(&game.prefix).join("system.reg"))
                    .unwrap()
                    .ino(),
                fs::metadata(root.image.join("system.reg")).unwrap().ino()
            );
        }
        fs::write(Path::new(&game.prefix).join("system.reg"), b"corrupt").unwrap();
        assert_eq!(
            fs::read(root.image.join("system.reg")).unwrap(),
            b"working registry"
        );
        assert_eq!(
            fs::read_link(root.image.join("host-link")).unwrap(),
            outside
        );
        assert!(image.exists());
    }

    #[test]
    fn deleted_current_saves_are_not_resurrected_and_first_session_saves_survive() {
        let (_temp, service, game) = fixture();
        fs::remove_dir_all(saves(&game)).unwrap();
        let id = service.create_checkpoint(game.id).unwrap();
        fs::create_dir_all(saves(&game)).unwrap();
        fs::write(saves(&game).join("first.ess"), b"first session").unwrap();
        service.restore_checkpoint(game.id, id).unwrap();
        assert_eq!(
            fs::read(saves(&game).join("first.ess")).unwrap(),
            b"first session"
        );
        let with_save = service.create_checkpoint(game.id).unwrap();
        fs::remove_dir_all(saves(&game)).unwrap();
        service.restore_checkpoint(game.id, with_save).unwrap();
        assert!(!saves(&game).exists());
    }

    #[test]
    fn external_documents_links_keep_host_saves_untouched() {
        let (temp, service, game) = fixture();
        let documents = Path::new(&game.prefix).join("drive_c/users/player/Documents");
        let external = temp.path().join("host Documents");
        fs::rename(&documents, &external).unwrap();
        symlink(&external, &documents).unwrap();
        let id = service.create_checkpoint(game.id).unwrap();
        fs::write(saves(&game).join("slot.ess"), b"external progress").unwrap();
        service.restore_checkpoint(game.id, id).unwrap();
        assert_eq!(
            fs::read(saves(&game).join("slot.ess")).unwrap(),
            b"external progress"
        );
        assert_eq!(fs::read_link(&documents).unwrap(), external);
    }

    #[test]
    fn saves_linked_between_room_trees_are_preserved_without_following_staged_links() {
        let (_temp, service, game) = fixture();
        let linked = Path::new(&game.executable)
            .parent()
            .unwrap()
            .join("save-storage");
        fs::rename(saves(&game), &linked).unwrap();
        symlink(&linked, saves(&game)).unwrap();
        let id = service.create_checkpoint(game.id).unwrap();
        fs::write(linked.join("slot.ess"), b"new progress").unwrap();
        service.restore_checkpoint(game.id, id).unwrap();
        assert_eq!(fs::read(linked.join("slot.ess")).unwrap(), b"new progress");
    }

    #[test]
    fn portable_mo2_profile_saves_and_tool_configuration_survive_restore() {
        let (temp, service, game) = fixture();
        let manager = temp.path().join("MO2");
        fs::create_dir_all(manager.join("profiles/main/saves")).unwrap();
        fs::write(manager.join("ModOrganizer.exe"), b"MZ manager").unwrap();
        fs::write(manager.join("profiles/main/saves/local.ess"), b"old").unwrap();
        let tool = service
            .save_tool(ToolSettings {
                id: None,
                game_id: game.id,
                kind: "mod_manager".into(),
                name: "MO2".into(),
                executable: manager.join("ModOrganizer.exe").to_string_lossy().into(),
                arguments: vec!["-p".into(), "main".into()],
                working_directory: manager.to_string_lossy().into(),
            })
            .unwrap();
        service
            .connection()
            .unwrap()
            .execute(
                "UPDATE games SET launch_via_tool_id=?1 WHERE id=?2",
                params![tool.id, game.id],
            )
            .unwrap();
        let id = service.create_checkpoint(game.id).unwrap();
        fs::write(manager.join("profiles/main/saves/local.ess"), b"new").unwrap();
        fs::create_dir_all(manager.join("profiles/new/saves")).unwrap();
        fs::write(manager.join("profiles/new/saves/slot.ess"), b"new profile").unwrap();
        service
            .connection()
            .unwrap()
            .execute("UPDATE tools SET name='Changed' WHERE id=?1", [tool.id])
            .unwrap();
        service.restore_checkpoint(game.id, id).unwrap();
        let game = service.recovery_game(game.id).unwrap();
        assert_eq!(game.tools[0].name, "MO2");
        assert_eq!(game.launch_via_tool_id, Some(game.tools[0].id));
        assert_eq!(
            fs::read(manager.join("profiles/main/saves/local.ess")).unwrap(),
            b"new"
        );
        assert_eq!(
            fs::read(manager.join("profiles/new/saves/slot.ess")).unwrap(),
            b"new profile"
        );
    }

    #[test]
    fn interrupted_restore_completes_after_every_rename_boundary() {
        for completed_renames in 0..=6 {
            let (temp, service, game) = fixture();
            let id = service.create_checkpoint(game.id).unwrap();
            let db = service.connection().unwrap();
            let json: String = db
                .query_row(
                    "SELECT manifest_json FROM room_checkpoints WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )
                .unwrap();
            let manifest: Manifest = serde_json::from_str(&json).unwrap();
            let mut switches = Vec::new();
            for (i, root) in manifest.roots.iter().enumerate() {
                let parent = root.live.parent().unwrap();
                let stage = parent.join(format!(".vessel-stage-{}-test-{i}", game.id));
                clone_tree(&root.image, &stage, false).unwrap();
                switches.push(Switch {
                    live: root.live.clone(),
                    stage,
                    previous: parent.join(format!(".vessel-previous-{}-test-{i}", game.id)),
                });
            }
            db.execute(
                "INSERT INTO room_restores(game_id,checkpoint_id,journal_json) VALUES (?1,?2,?3)",
                params![
                    game.id,
                    id,
                    serde_json::to_string(&Journal { switches }).unwrap()
                ],
            )
            .unwrap();
            let journal: String = db
                .query_row("SELECT journal_json FROM room_restores", [], |r| r.get(0))
                .unwrap();
            let journal: Journal = serde_json::from_str(&journal).unwrap();
            for (i, switch) in journal.switches.iter().enumerate() {
                if i * 2 < completed_renames {
                    fs::rename(&switch.live, &switch.previous).unwrap();
                }
                if i * 2 + 1 < completed_renames {
                    fs::rename(&switch.stage, &switch.live).unwrap();
                }
            }
            drop(db);
            drop(service);
            let service =
                Launcher::at(temp.path().join("state"), temp.path().join("logs")).unwrap();
            assert_eq!(
                service
                    .connection()
                    .unwrap()
                    .query_row("SELECT count(*) FROM room_restores", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            assert_eq!(
                fs::read(Path::new(&game.prefix).join("system.reg")).unwrap(),
                b"working registry"
            );
            assert_eq!(
                fs::read(
                    Path::new(&game.executable)
                        .parent()
                        .unwrap()
                        .join("mod.dll")
                )
                .unwrap(),
                b"working mod"
            );
        }
    }

    #[test]
    fn refuses_unknown_games_cross_room_checkpoints_and_shared_folders() {
        let (temp, service, game) = fixture();
        let unknown = temp.path().join("other");
        fs::create_dir(&unknown).unwrap();
        fs::write(unknown.join("Unknown.exe"), b"MZ game").unwrap();
        let other = service
            .import(&unknown.join("Unknown.exe"), None, "manual")
            .unwrap();
        assert!(service.create_checkpoint(other.id).is_err());
        let id = service.create_checkpoint(game.id).unwrap();
        assert!(service.restore_checkpoint(other.id, id).is_err());
        service
            .save_tool(ToolSettings {
                id: None,
                game_id: other.id,
                kind: "utility".into(),
                name: "Shared tool".into(),
                executable: game.executable.clone(),
                arguments: vec![],
                working_directory: Path::new(&game.executable)
                    .parent()
                    .unwrap()
                    .to_string_lossy()
                    .into(),
            })
            .unwrap();
        assert!(service
            .create_checkpoint(game.id)
            .unwrap_err()
            .contains("shares a game or companion folder"));
    }

    #[test]
    fn library_lock_blocks_a_second_writer_and_session_retention_cleans_storage() {
        let (temp, service, game) = fixture();
        assert!(Launcher::at(temp.path().join("state"), temp.path().join("logs")).is_err());
        let first = service.checkpoint_room(&game, "Before session").unwrap();
        for _ in 0..3 {
            service.checkpoint_room(&game, "Before session").unwrap();
        }
        assert_eq!(
            service
                .recovery_game(game.id)
                .unwrap()
                .recovery
                .checkpoints
                .len(),
            3
        );
        assert!(!service
            .data
            .join(format!("checkpoints/game-{}/{first}", game.id))
            .exists());
    }

    #[test]
    fn automatic_checkpoint_precedes_first_process_and_live_room_is_locked() {
        let (temp, service, game) = fixture();
        let runner = temp.path().join("wine-test");
        fs::write(&runner, "#!/bin/sh\nsleep 0.4\n").unwrap();
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
        service
            .connection()
            .unwrap()
            .execute(
                "UPDATE games SET runtime_path=?1 WHERE id=?2",
                params![runner.to_string_lossy(), game.id],
            )
            .unwrap();
        service.launch(game.id).unwrap();
        let game = service.recovery_game(game.id).unwrap();
        assert_eq!(game.recovery.checkpoints.len(), 1);
        assert!(service
            .create_checkpoint(game.id)
            .unwrap_err()
            .contains("Close the game"));
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    #[test]
    fn restoring_archives_new_profiles_without_deleting_or_reclassifying_history() {
        let (_temp, service, game) = fixture();
        let id = service.create_checkpoint(game.id).unwrap();
        let tool = service
            .save_tool(ToolSettings {
                id: None,
                game_id: game.id,
                kind: "script_extender".into(),
                name: "SKSE".into(),
                executable: game.executable.clone(),
                arguments: vec![],
                working_directory: Path::new(&game.executable)
                    .parent()
                    .unwrap()
                    .to_string_lossy()
                    .into(),
            })
            .unwrap();
        service.connection().unwrap().execute("INSERT INTO launches(game_id,tool_id,state,started_at,ended_at) VALUES (?1,?2,'finished','2026-10-24 10:00:00','2026-10-24 10:01:00')", params![game.id,tool.id]).unwrap();
        service.restore_checkpoint(game.id, id).unwrap();
        let restored = service.recovery_game(game.id).unwrap();
        assert!(restored.tools.is_empty());
        assert_eq!(restored.play_seconds, 60);
        assert_eq!(restored.sessions[0].via.as_deref(), Some("SKSE"));
        assert_eq!(
            service
                .connection()
                .unwrap()
                .query_row("SELECT count(*) FROM launches", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn detects_unrecorded_wine_children_and_reclaims_failed_preparation() {
        let (temp, service, game) = fixture();
        let mut child = Command::new("sleep")
            .arg("5")
            .env("WINEPREFIX", &game.prefix)
            .spawn()
            .unwrap();
        let result = service.create_checkpoint(game.id);
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(result.unwrap_err().contains("Wine process"));
        let abandoned = service.data.join("checkpoints/abandoned/root-0");
        fs::create_dir_all(&abandoned).unwrap();
        fs::write(abandoned.join("partial"), b"incomplete").unwrap();
        service
            .connection()
            .unwrap()
            .execute(
                "INSERT INTO room_storage_garbage(path) VALUES (?1)",
                [abandoned.to_string_lossy().as_ref()],
            )
            .unwrap();
        drop(service);
        let _service = Launcher::at(temp.path().join("state"), temp.path().join("logs")).unwrap();
        assert!(!abandoned.exists());
    }

    #[test]
    fn arbitrary_game_profile_preserves_game_and_home_saves_and_restores_home_and_policy() {
        let (_temp, service, game) = fixture();
        let exe = Path::new(&game.executable).with_file_name("ArbitraryGame.exe");
        fs::rename(&game.executable, &exe).unwrap();
        service
            .connection()
            .unwrap()
            .execute(
                "UPDATE games SET executable=?1 WHERE id=?2",
                params![exe.to_string_lossy(), game.id],
            )
            .unwrap();
        let profile = crate::rooms::RoomProfile {
            network: "offline".into(),
            display: "headless".into(),
            game_save_paths: vec!["progress".into()],
            home_save_paths: vec!["save-data".into()],
            ..Default::default()
        };
        service.configure_room(game.id, profile).unwrap();
        let home = service.data.join(format!("rooms/game-{}/home", game.id));
        fs::create_dir_all(home.join("save-data")).unwrap();
        fs::create_dir_all(exe.parent().unwrap().join("progress")).unwrap();
        fs::write(home.join("settings.ini"), "healthy home settings").unwrap();
        fs::write(home.join("save-data/slot"), "old home progress").unwrap();
        fs::write(
            exe.parent().unwrap().join("progress/slot"),
            "old game progress",
        )
        .unwrap();
        let id = service.create_checkpoint(game.id).unwrap();
        fs::write(home.join("settings.ini"), "POISONED HOME SETTINGS").unwrap();
        fs::write(home.join("save-data/slot"), "new home progress").unwrap();
        fs::write(
            exe.parent().unwrap().join("progress/slot"),
            "new game progress",
        )
        .unwrap();
        let mut profile = service.recovery_game(game.id).unwrap().room.profile;
        profile.network = "online".into();
        service.configure_room(game.id, profile).unwrap();
        service.restore_checkpoint(game.id, id).unwrap();
        assert_eq!(
            fs::read_to_string(home.join("settings.ini")).unwrap(),
            "healthy home settings"
        );
        assert_eq!(
            fs::read_to_string(home.join("save-data/slot")).unwrap(),
            "new home progress"
        );
        assert_eq!(
            fs::read_to_string(exe.parent().unwrap().join("progress/slot")).unwrap(),
            "new game progress"
        );
        let game = service.recovery_game(game.id).unwrap();
        assert!(game.recovery.supported);
        assert_eq!(game.room.profile.network, "offline");
    }
}
