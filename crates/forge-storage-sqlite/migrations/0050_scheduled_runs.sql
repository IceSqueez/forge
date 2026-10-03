CREATE TABLE IF NOT EXISTS scheduled_runs (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    target_action_id    TEXT    NOT NULL,
    due_at              INTEGER NOT NULL,
    key                 TEXT,
    missed_policy       TEXT    NOT NULL CHECK (missed_policy IN ('run_late_once', 'skip_if_late_by')),
    skip_late_ms        INTEGER,
    args                TEXT    NOT NULL,
    scheduled_by_action TEXT,
    scheduled_by_run    TEXT,
    trigger_event_id    TEXT,
    scheduled_at        INTEGER NOT NULL,
    label               TEXT    NOT NULL,
    state               TEXT    NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'dispatched', 'cancelled', 'skipped', 'failed')),
    outcome_reason      TEXT,
    resolved_at         INTEGER,
    CHECK ((missed_policy = 'skip_if_late_by') = (skip_late_ms IS NOT NULL)),
    CHECK ((state = 'pending') = (resolved_at IS NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS scheduled_runs_pending_key
    ON scheduled_runs(key) WHERE state = 'pending' AND key IS NOT NULL;

CREATE INDEX IF NOT EXISTS scheduled_runs_pending_due
    ON scheduled_runs(due_at, id) WHERE state = 'pending';

CREATE INDEX IF NOT EXISTS scheduled_runs_pending_target
    ON scheduled_runs(target_action_id) WHERE state = 'pending';

CREATE INDEX IF NOT EXISTS scheduled_runs_resolved
    ON scheduled_runs(resolved_at) WHERE state <> 'pending';

CREATE TRIGGER IF NOT EXISTS scheduled_runs_action_deleted
AFTER DELETE ON actions
BEGIN
    DELETE FROM scheduled_runs WHERE target_action_id = OLD.id AND state = 'pending';
END;
