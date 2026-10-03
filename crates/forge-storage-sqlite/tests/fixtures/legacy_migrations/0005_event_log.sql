CREATE TABLE event_log (
    id        TEXT    PRIMARY KEY NOT NULL,
    source    TEXT    NOT NULL,
    kind      TEXT    NOT NULL,
    timestamp INTEGER NOT NULL,
    payload   TEXT    NOT NULL DEFAULT '{}',
    caused_by TEXT,
    replay    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_event_log_timestamp ON event_log (timestamp);
CREATE INDEX idx_event_log_source_kind ON event_log (source, kind);
