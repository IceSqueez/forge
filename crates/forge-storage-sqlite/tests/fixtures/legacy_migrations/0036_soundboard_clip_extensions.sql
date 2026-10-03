ALTER TABLE soundboard_clips ADD COLUMN category TEXT NOT NULL DEFAULT '';
ALTER TABLE soundboard_clips ADD COLUMN loop_playback INTEGER NOT NULL DEFAULT 0;
ALTER TABLE soundboard_clips ADD COLUMN duration_secs REAL;
ALTER TABLE soundboard_clips ADD COLUMN builtin_id TEXT;

CREATE INDEX IF NOT EXISTS idx_soundboard_clips_category ON soundboard_clips(category);
