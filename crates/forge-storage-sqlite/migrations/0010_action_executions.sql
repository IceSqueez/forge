CREATE TABLE action_executions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    action_id       TEXT NOT NULL,
    started_at      INTEGER NOT NULL,
    duration_ms     INTEGER NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('ok','err')),
    error_message   TEXT
);

CREATE INDEX idx_action_executions_action_started
    ON action_executions(action_id, started_at DESC);

CREATE INDEX idx_action_executions_started
    ON action_executions(started_at);
