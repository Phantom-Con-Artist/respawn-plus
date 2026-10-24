// SPDX-License-Identifier: MIT
//! Vessel was called Respawn+ and kept its library in `respawn-plus` folders. The first time Vessel
//! starts, it moves that library (games, prefixes, saves, checkpoints, history and logs) into its
//! own folders, so nothing is re-imported. Each step can be repeated, so an interrupted move
//! finishes on the next start.
use crate::database;
use std::{
    fs::{self, File},
    path::Path,
};

const FORMER: &str = "respawn-plus";
pub const NAME: &str = "vessel";

/// `data_root` and `state_root` are the platform data and state directories (they may be equal).
pub fn adopt_library(data_root: &Path, state_root: &Path) -> Result<(), String> {
    let (old_data, new_data) = (data_root.join(FORMER), data_root.join(NAME));
    if new_data.exists() || !old_data.is_dir() {
        return Ok(());
    }
    // The former launcher holds this lock while it runs; never move a library that is in use.
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(old_data.join("launcher.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock().map_err(|_| {
        "Close Respawn+ first: Vessel is moving its library into Vessel's folder".to_string()
    })?;
    let (old_state, new_state) = (state_root.join(FORMER), state_root.join(NAME));
    let old_db = old_data.join(format!("{FORMER}.sqlite3"));
    if old_db.is_file() {
        let db = database::open(&old_db).map_err(|e| e.to_string())?;
        let mut moves = vec![(old_data.as_path(), new_data.as_path())];
        if old_state != old_data {
            moves.push((old_state.as_path(), new_state.as_path()));
        }
        rewrite_paths(&db, &moves)?;
        // Fold the write-ahead log into the file so the database is a single file to rename.
        db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|e| e.to_string())?;
        drop(db);
        for suffix in ["-wal", "-shm"] {
            let _ = fs::remove_file(old_data.join(format!("{FORMER}.sqlite3{suffix}")));
        }
        fs::rename(&old_db, old_data.join(format!("{NAME}.sqlite3"))).map_err(|e| e.to_string())?;
    }
    if old_state != old_data && old_state.is_dir() && !new_state.exists() {
        fs::rename(&old_state, &new_state).map_err(|e| e.to_string())?;
    }
    // Last, because the existence of the new data folder marks the move as complete.
    fs::rename(&old_data, &new_data).map_err(|e| e.to_string())?;
    Ok(())
}

/// Points every stored path below a moved folder at its new location, in every text column of
/// every table, so the rewrite needs no knowledge of the schema.
fn rewrite_paths(db: &rusqlite::Connection, moves: &[(&Path, &Path)]) -> Result<(), String> {
    let err = |e: rusqlite::Error| e.to_string();
    let tables: Vec<String> = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .map_err(err)?
        .query_map([], |row| row.get(0))
        .map_err(err)?
        .collect::<Result<_, _>>()
        .map_err(err)?;
    let transaction = db.unchecked_transaction().map_err(err)?;
    for table in &tables {
        let columns: Vec<String> = transaction
            .prepare("SELECT name FROM pragma_table_info(?1)")
            .map_err(err)?
            .query_map([table], |row| row.get(0))
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        for column in &columns {
            let (t, c) = (table.replace('"', "\"\""), column.replace('"', "\"\""));
            for (old, new) in moves {
                let (old, new) = (old.display().to_string(), new.display().to_string());
                transaction
                    .execute(
                        &format!("UPDATE \"{t}\" SET \"{c}\" = ?2 WHERE \"{c}\" = ?1"),
                        [&old, &new],
                    )
                    .map_err(err)?;
                transaction
                    .execute(
                        &format!("UPDATE \"{t}\" SET \"{c}\" = replace(\"{c}\", ?1, ?2) WHERE typeof(\"{c}\") = 'text' AND instr(\"{c}\", ?1) > 0"),
                        [format!("{old}/"), format!("{new}/")],
                    )
                    .map_err(err)?;
            }
        }
    }
    transaction.commit().map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn former_library(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
        let (data, state) = (root.join("share"), root.join("state"));
        let old = data.join(FORMER);
        fs::create_dir_all(old.join("prefixes/game-1/pfx")).unwrap();
        fs::write(old.join("prefixes/game-1/pfx/system.reg"), "kept").unwrap();
        fs::create_dir_all(state.join(FORMER).join("logs/library/game-1")).unwrap();
        fs::write(
            state.join(FORMER).join("logs/library/game-1/launch-1.log"),
            "log",
        )
        .unwrap();
        let db = database::open(&old.join(format!("{FORMER}.sqlite3"))).unwrap();
        db.execute(
            "INSERT INTO games (name,install_path,executable) VALUES ('Any',?1,?2)",
            [
                "/games/Any".to_string(),
                old.join("test-games/Game.exe").display().to_string(),
            ],
        )
        .unwrap();
        db.execute(
            "INSERT INTO prefixes (game_id,path) VALUES (1,?1)",
            [old.join("prefixes/game-1").display().to_string()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO launches (game_id,state,log_path) VALUES (1,'finished',?1)",
            [state
                .join(FORMER)
                .join("logs/library/game-1/launch-1.log")
                .display()
                .to_string()],
        )
        .unwrap();
        (data, state)
    }

    #[test]
    fn moves_the_former_library_and_every_stored_path_once() {
        let temp = tempfile::tempdir().unwrap();
        let (data, state) = former_library(temp.path());
        adopt_library(&data, &state).unwrap();
        let new = data.join(NAME);
        assert!(!data.join(FORMER).exists() && !state.join(FORMER).exists());
        assert_eq!(
            fs::read_to_string(new.join("prefixes/game-1/pfx/system.reg")).unwrap(),
            "kept"
        );
        let db = database::open(&new.join(format!("{NAME}.sqlite3"))).unwrap();
        let text = |sql: &str| db.query_row(sql, [], |r| r.get::<_, String>(0)).unwrap();
        assert_eq!(text("SELECT install_path FROM games"), "/games/Any");
        assert_eq!(
            text("SELECT executable FROM games"),
            new.join("test-games/Game.exe").display().to_string()
        );
        assert_eq!(
            text("SELECT path FROM prefixes"),
            new.join("prefixes/game-1").display().to_string()
        );
        let log = text("SELECT log_path FROM launches");
        assert_eq!(
            log,
            state
                .join(NAME)
                .join("logs/library/game-1/launch-1.log")
                .display()
                .to_string()
        );
        assert_eq!(fs::read_to_string(log).unwrap(), "log");
        // Already moved: nothing else happens.
        adopt_library(&data, &state).unwrap();
        assert!(new.join(format!("{NAME}.sqlite3")).is_file());
    }

    #[test]
    fn leaves_a_library_alone_while_the_former_launcher_has_it_open() {
        let temp = tempfile::tempdir().unwrap();
        let (data, state) = former_library(temp.path());
        let held = File::create(data.join(FORMER).join("launcher.lock")).unwrap();
        held.try_lock().unwrap();
        assert!(adopt_library(&data, &state).is_err());
        assert!(data
            .join(FORMER)
            .join(format!("{FORMER}.sqlite3"))
            .is_file());
        assert!(!data.join(NAME).exists());
    }

    #[test]
    fn finishes_a_move_interrupted_after_the_database_was_renamed() {
        let temp = tempfile::tempdir().unwrap();
        let (data, state) = former_library(temp.path());
        let old = data.join(FORMER);
        let db = database::open(&old.join(format!("{FORMER}.sqlite3"))).unwrap();
        rewrite_paths(&db, &[(old.as_path(), data.join(NAME).as_path())]).unwrap();
        drop(db);
        fs::rename(
            old.join(format!("{FORMER}.sqlite3")),
            old.join(format!("{NAME}.sqlite3")),
        )
        .unwrap();
        adopt_library(&data, &state).unwrap();
        assert!(data.join(NAME).join(format!("{NAME}.sqlite3")).is_file());
        assert!(state.join(NAME).join("logs").is_dir());
    }
}
