CREATE TABLE IF NOT EXISTS scripts (
    id            TEXT PRIMARY KEY,
    name          TEXT UNIQUE NOT NULL,
    source_code   TEXT NOT NULL,
    description   TEXT,
    enabled       INTEGER NOT NULL,
    created_at    INTEGER NOT NULL,
    last_modified INTEGER NOT NULL
);

CREATE INDEX scripts_enabled ON scripts(enabled);
