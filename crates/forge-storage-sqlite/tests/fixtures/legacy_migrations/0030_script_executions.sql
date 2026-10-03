CREATE TABLE script_executions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    script_id       TEXT NOT NULL,
    started_at      INTEGER NOT NULL,
    duration_ms     INTEGER NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('ok','err'))
);

CREATE INDEX idx_script_executions_script_started
    ON script_executions(script_id, started_at DESC);

CREATE INDEX idx_script_executions_started
    ON script_executions(started_at);
