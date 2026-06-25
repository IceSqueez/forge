-- Ordered TTS filter rules.
-- position is dense 0..n by convention; gaps are a load-time repair in the TTS domain.
CREATE TABLE tts_filter_rules (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL DEFAULT '',
    enabled     INTEGER NOT NULL DEFAULT 1,
    position    INTEGER NOT NULL,
    kind        TEXT NOT NULL,   -- 'literal' | 'regex' | 'blocklist'
    params      TEXT NOT NULL    -- serde_json of the kind-specific fields
);

CREATE INDEX ix_tts_filter_rules_position ON tts_filter_rules(position ASC);

-- Singleton pipeline settings row (id = 1, always present after migration).
CREATE TABLE tts_pipeline_settings (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    url_mode            TEXT NOT NULL DEFAULT 'speak',
    max_length          INTEGER,                             -- NULL = unlimited
    blocklist_mode      TEXT NOT NULL DEFAULT 'censor',
    strip_twitch_emotes INTEGER NOT NULL DEFAULT 1,
    strip_reward_emotes INTEGER NOT NULL DEFAULT 1
);

INSERT OR IGNORE INTO tts_pipeline_settings
    (id, url_mode, max_length, blocklist_mode, strip_twitch_emotes, strip_reward_emotes)
VALUES
    (1, 'speak', NULL, 'censor', 1, 1);
