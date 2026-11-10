CREATE TABLE IF NOT EXISTS follow_knowledge (
    platform    TEXT    NOT NULL,
    viewer_id   TEXT    NOT NULL,
    status      TEXT    NOT NULL CHECK (status IN ('following', 'not_following', 'hidden')),
    followed_at INTEGER,
    observed_at INTEGER NOT NULL,
    fresh_until INTEGER NOT NULL,
    PRIMARY KEY (platform, viewer_id),
    CHECK ((status = 'following') = (followed_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_follow_knowledge_fresh_until
    ON follow_knowledge(fresh_until);

CREATE TABLE IF NOT EXISTS follow_knowledge_meta (
    id     INTEGER PRIMARY KEY CHECK (id = 1),
    seeded INTEGER NOT NULL CHECK (seeded IN (0, 1))
);

INSERT OR IGNORE INTO follow_knowledge_meta (id, seeded) VALUES (1, 0);
