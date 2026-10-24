// SPDX-License-Identifier: MIT
use rusqlite::Connection;
use std::{path::Path, time::Duration};

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let mut connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    migrate(&mut connection)?;
    Ok(connection)
}

fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    let version: u32 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > 5 {
        return Err(rusqlite::Error::InvalidParameterName(
            "Database is newer than this version of Respawn+".into(),
        ));
    }
    if version == 0 {
        transaction.execute_batch(include_str!("schema.sql"))?;
        transaction.pragma_update(None, "user_version", 1)?;
    }
    if version < 2 {
        transaction.execute_batch(include_str!("schema-v2.sql"))?;
        transaction.pragma_update(None, "user_version", 2)?;
    }
    if version < 3 {
        transaction.execute_batch(include_str!("schema-v3.sql"))?;
        transaction.pragma_update(None, "user_version", 3)?;
    }
    if version < 4 {
        transaction.execute_batch(include_str!("schema-v4.sql"))?;
        transaction.pragma_update(None, "user_version", 4)?;
    }
    if version < 5 {
        transaction.execute_batch(include_str!("schema-v5.sql"))?;
        transaction.pragma_update(None, "user_version", 5)?;
    }
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_persists_and_game_deletion_cleans_up_tools() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        {
            let db = open(&path).unwrap();
            db.execute(
                "INSERT INTO games (name, install_path) VALUES ('Skyrim', '/games/skyrim')",
                [],
            )
            .unwrap();
            db.execute("INSERT INTO tools (game_id, name, executable, kind) VALUES (1, 'Trainer', '/tools/trainer.exe', 'trainer')", []).unwrap();
            assert!(db.execute("INSERT INTO mods (game_id, name, source_path) VALUES (999, 'Invalid', '/mods')", []).is_err());
        }
        let db = open(&path).unwrap();
        let name: String = db
            .query_row("SELECT name FROM games", [], |row| row.get(0))
            .unwrap();
        assert_eq!(name, "Skyrim");
        db.execute("DELETE FROM games WHERE id = 1", []).unwrap();
        let tools: u32 = db
            .query_row("SELECT COUNT(*) FROM tools", [], |row| row.get(0))
            .unwrap();
        assert_eq!(tools, 0);
    }

    #[test]
    fn upgrades_existing_library_without_losing_game_records() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("schema.sql")).unwrap();
        db.pragma_update(None, "user_version", 1).unwrap();
        db.execute(
            "INSERT INTO games (name,install_path) VALUES ('Existing game','/existing')",
            [],
        )
        .unwrap();
        migrate(&mut db).unwrap();
        let row: (String, String) = db
            .query_row("SELECT name,runtime_kind FROM games", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(row, ("Existing game".into(), "proton".into()));
    }

    #[test]
    fn v3_preserves_existing_tools_and_game_launch_history() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("schema.sql")).unwrap();
        db.execute_batch(include_str!("schema-v2.sql")).unwrap();
        db.pragma_update(None, "user_version", 2).unwrap();
        db.execute(
            "INSERT INTO games (name,install_path) VALUES ('Game','/game')",
            [],
        )
        .unwrap();
        db.execute("INSERT INTO tools (game_id,name,executable,kind) VALUES (1,'MO2','/manager/ModOrganizer.exe','mod_manager')", []).unwrap();
        db.execute(
            "INSERT INTO launches (game_id,state,exit_code) VALUES (1,'finished',0)",
            [],
        )
        .unwrap();
        migrate(&mut db).unwrap();
        let directory: String = db
            .query_row("SELECT working_directory FROM tools", [], |r| r.get(0))
            .unwrap();
        let tool_id: Option<i64> = db
            .query_row("SELECT tool_id FROM launches", [], |r| r.get(0))
            .unwrap();
        assert!(directory.is_empty());
        assert!(tool_id.is_none());
    }

    #[test]
    fn migration_is_repeatable_and_rejects_newer_databases() {
        let mut db = Connection::open_in_memory().unwrap();
        migrate(&mut db).unwrap();
        migrate(&mut db).unwrap();
        db.pragma_update(None, "user_version", 6).unwrap();
        assert!(migrate(&mut db).is_err());
    }
}
