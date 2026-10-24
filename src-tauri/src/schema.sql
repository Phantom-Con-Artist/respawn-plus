-- SPDX-License-Identifier: MIT
CREATE TABLE IF NOT EXISTS games (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    install_path TEXT NOT NULL UNIQUE,
    executable TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS runtimes (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS prefixes (
    id INTEGER PRIMARY KEY,
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    path TEXT NOT NULL UNIQUE,
    runtime_id INTEGER REFERENCES runtimes(id)
);
CREATE TABLE IF NOT EXISTS mods (
    id INTEGER PRIMARY KEY,
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    source_path TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1))
);
CREATE TABLE IF NOT EXISTS launch_profiles (
    id INTEGER PRIMARY KEY,
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    arguments TEXT NOT NULL DEFAULT '',
    environment_json TEXT NOT NULL DEFAULT '{}'
);
-- Companion apps execute inside their game's selected runtime and prefix.
CREATE TABLE IF NOT EXISTS tools (
    id INTEGER PRIMARY KEY,
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    executable TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('mod_manager', 'script_extender', 'trainer', 'utility')),
    arguments_json TEXT NOT NULL DEFAULT '[]',
    environment_json TEXT NOT NULL DEFAULT '{}'
);
