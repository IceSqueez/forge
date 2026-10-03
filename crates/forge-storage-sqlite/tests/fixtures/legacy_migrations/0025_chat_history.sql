CREATE TABLE chat_history (
    id            TEXT    PRIMARY KEY NOT NULL,
    event_id      TEXT    NOT NULL,
    source        TEXT    NOT NULL,
    received_at   INTEGER NOT NULL,
    author        TEXT    NOT NULL,
    author_color  TEXT,
    body_segments TEXT    NOT NULL DEFAULT '[]',
    badges        TEXT    NOT NULL DEFAULT '[]',
    is_event      INTEGER NOT NULL DEFAULT 0,
    event_detail  TEXT,
    moderation    TEXT    NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_chat_history_received_at ON chat_history (received_at);
