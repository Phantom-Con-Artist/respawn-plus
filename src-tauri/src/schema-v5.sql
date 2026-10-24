-- SPDX-License-Identifier: MIT
-- Play can start a companion app (a mod manager) that starts the game; such launches count as play.
ALTER TABLE games ADD COLUMN launch_via_tool_id INTEGER REFERENCES tools(id) ON DELETE SET NULL;
ALTER TABLE launches ADD COLUMN as_play INTEGER NOT NULL DEFAULT 0;
