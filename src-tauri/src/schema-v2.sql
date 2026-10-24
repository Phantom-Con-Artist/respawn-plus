-- SPDX-License-Identifier: MIT
ALTER TABLE games ADD COLUMN source TEXT NOT NULL DEFAULT 'manual';
ALTER TABLE games ADD COLUMN runtime_kind TEXT NOT NULL DEFAULT 'proton';
ALTER TABLE games ADD COLUMN runtime_path TEXT NOT NULL DEFAULT '';
ALTER TABLE games ADD COLUMN arguments_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE games ADD COLUMN last_played_at TEXT;

CREATE TABLE launches (
    id INTEGER PRIMARY KEY,
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    pid INTEGER,
    process_token TEXT,
    state TEXT NOT NULL CHECK(state IN ('starting', 'running', 'finished', 'failed', 'unknown')),
    exit_code INTEGER,
    log_path TEXT NOT NULL DEFAULT '',
    error TEXT,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ended_at TEXT
);
CREATE INDEX launches_game_id ON launches(game_id, id DESC);
