CREATE TABLE IF NOT EXISTS viewers (
    platform        TEXT    NOT NULL,
    viewer_id       TEXT    NOT NULL,
    username        TEXT    NOT NULL,
    first_seen_at   INTEGER NOT NULL,
    last_seen_at    INTEGER NOT NULL,
    message_count   INTEGER NOT NULL DEFAULT 0,
    custom_greeting INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (platform, viewer_id)
);

CREATE INDEX IF NOT EXISTS viewers_last_seen
    ON viewers(last_seen_at DESC);

CREATE INDEX IF NOT EXISTS viewers_by_username
    ON viewers(username COLLATE NOCASE);
