-- SPDX-License-Identifier: MIT
CREATE TABLE room_profiles (
    game_id INTEGER PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
    profile_json TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE windows_runtime_sources (
    digest TEXT PRIMARY KEY,
    source_path TEXT NOT NULL
);
