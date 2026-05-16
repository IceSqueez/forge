CREATE TABLE IF NOT EXISTS actions (
    id            TEXT    PRIMARY KEY,
    name          TEXT    UNIQUE NOT NULL,
    config_json   TEXT    NOT NULL,
    created_at    INTEGER NOT NULL,
    last_modified INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS triggers (
    id            TEXT    PRIMARY KEY,
    name          TEXT    NOT NULL,
    source        TEXT    NOT NULL,
    pattern_json  TEXT    NOT NULL,
    action_id     TEXT    NOT NULL REFERENCES actions(id) ON DELETE CASCADE,
    enabled       INTEGER NOT NULL,
    created_at    INTEGER NOT NULL,
    last_modified INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS triggers_source_enabled ON triggers(source, enabled);

CREATE TABLE IF NOT EXISTS commands (
    id            TEXT    PRIMARY KEY,
    name          TEXT    UNIQUE NOT NULL,
    action_id     TEXT    NOT NULL REFERENCES actions(id) ON DELETE CASCADE,
    cooldown_ms   INTEGER NOT NULL,
    permission    TEXT    NOT NULL,
    enabled       INTEGER NOT NULL,
    created_at    INTEGER NOT NULL,
    last_modified INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS queues (
    id            TEXT    PRIMARY KEY,
    name          TEXT    UNIQUE NOT NULL,
    blocking      INTEGER NOT NULL,
    enabled       INTEGER NOT NULL,
    paused        INTEGER NOT NULL,
    created_at    INTEGER NOT NULL,
    last_modified INTEGER NOT NULL
);
