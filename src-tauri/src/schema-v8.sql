-- SPDX-License-Identifier: MIT
-- A trainer or utility can open with the game whenever Play starts it.
ALTER TABLE tools ADD COLUMN with_game INTEGER NOT NULL DEFAULT 0;
