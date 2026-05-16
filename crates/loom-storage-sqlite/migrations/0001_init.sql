CREATE TABLE IF NOT EXISTS globals (
    name          TEXT    PRIMARY KEY,
    value         TEXT    NOT NULL,
    type_tag      TEXT    NOT NULL,
    persisted     INTEGER NOT NULL DEFAULT 1,
    reads         INTEGER NOT NULL DEFAULT 0,
    writes        INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    last_modified INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS globals_persisted ON globals(persisted);

CREATE TABLE IF NOT EXISTS user_globals (
    broadcaster_id TEXT    NOT NULL,
    user_id        TEXT    NOT NULL,
    name           TEXT    NOT NULL,
    value          TEXT    NOT NULL,
    type_tag       TEXT    NOT NULL,
    last_modified  INTEGER NOT NULL,
    PRIMARY KEY (broadcaster_id, user_id, name)
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS action_history (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    action_id           TEXT    NOT NULL,
    triggering_event_id TEXT,
    started_at          INTEGER NOT NULL,
    duration_ms         INTEGER NOT NULL,
    outcome             TEXT    NOT NULL,
    context             TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS action_history_by_action
    ON action_history(action_id, started_at DESC);
CREATE INDEX IF NOT EXISTS action_history_by_event
    ON action_history(triggering_event_id)
    WHERE triggering_event_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS credentials (
    id           TEXT PRIMARY KEY,
    encrypted    BLOB NOT NULL,
    nonce        BLOB NOT NULL,
    last_refresh INTEGER NOT NULL
);
