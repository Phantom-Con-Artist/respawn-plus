-- SPDX-License-Identifier: MIT
ALTER TABLE games ADD COLUMN display_mode TEXT NOT NULL DEFAULT 'default' CHECK(display_mode IN ('default', 'fullscreen', 'borderless', 'windowed'));
ALTER TABLE games ADD COLUMN display_width INTEGER;
ALTER TABLE games ADD COLUMN display_height INTEGER;
