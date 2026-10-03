CREATE TABLE IF NOT EXISTS soundboard_clips (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    volume REAL NOT NULL DEFAULT 1.0,
    output_device TEXT NOT NULL,
    hotkey TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_soundboard_clips_name ON soundboard_clips(name);
