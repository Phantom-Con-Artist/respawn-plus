-- SPDX-License-Identifier: MIT
-- Archive profiles removed by a restore: their launch history must survive.
ALTER TABLE tools ADD COLUMN recovery_archived INTEGER NOT NULL DEFAULT 0 CHECK(recovery_archived IN (0,1));
ALTER TABLE launches ADD COLUMN companion_kind TEXT;
ALTER TABLE launches ADD COLUMN companion_name TEXT;
UPDATE launches SET companion_kind=(SELECT kind FROM tools WHERE id=launches.tool_id),
    companion_name=(SELECT name FROM tools WHERE id=launches.tool_id) WHERE tool_id IS NOT NULL;
CREATE TABLE room_checkpoints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('preparing','ready')),
    manifest_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX room_checkpoints_game ON room_checkpoints(game_id, id DESC);
-- A durable intent record bridges SQLite transactions and filesystem renames.
CREATE TABLE room_restores (
    game_id INTEGER PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
    checkpoint_id INTEGER NOT NULL REFERENCES room_checkpoints(id),
    journal_json TEXT NOT NULL
);
CREATE TABLE room_storage_garbage (path TEXT PRIMARY KEY);
