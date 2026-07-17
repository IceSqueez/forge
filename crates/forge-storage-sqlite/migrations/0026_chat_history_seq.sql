CREATE TABLE chat_history_new (
    seq           INTEGER PRIMARY KEY AUTOINCREMENT,
    id            TEXT    NOT NULL UNIQUE,
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

INSERT INTO chat_history_new
    (id, event_id, source, received_at, author, author_color,
     body_segments, badges, is_event, event_detail, moderation)
SELECT id, event_id, source, received_at, author, author_color,
       body_segments, badges, is_event, event_detail, moderation
FROM chat_history
ORDER BY received_at;

DROP TABLE chat_history;

ALTER TABLE chat_history_new RENAME TO chat_history;

CREATE INDEX idx_chat_history_received_at ON chat_history (received_at);
CREATE INDEX idx_chat_history_source_author ON chat_history (source, author);
