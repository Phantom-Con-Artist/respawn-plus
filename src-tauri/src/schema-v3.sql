-- SPDX-License-Identifier: MIT
ALTER TABLE tools ADD COLUMN working_directory TEXT NOT NULL DEFAULT '';
ALTER TABLE launches ADD COLUMN tool_id INTEGER REFERENCES tools(id) ON DELETE CASCADE;
CREATE INDEX launches_tool_id ON launches(tool_id, id DESC);
