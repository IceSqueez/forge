CREATE TABLE voice_aliases (
    id              TEXT PRIMARY KEY,
    viewer_id       TEXT NOT NULL,
    viewer_name     TEXT NOT NULL,
    engine_id       TEXT NOT NULL,
    voice_id        TEXT NOT NULL,
    pitch_semitones REAL,
    rate_multiplier REAL,
    state           TEXT NOT NULL DEFAULT 'Active',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX ix_voice_aliases_viewer ON voice_aliases(viewer_id);

CREATE TABLE ignore_profile (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    excluded_voice_ids  TEXT NOT NULL DEFAULT '[]',
    excluded_locales    TEXT NOT NULL DEFAULT '[]',
    updated_at          TEXT NOT NULL
);

INSERT OR IGNORE INTO ignore_profile(id, excluded_voice_ids, excluded_locales, updated_at)
VALUES (1, '[]', '[]', datetime('now'));

CREATE TABLE replacement_rules (
    id          TEXT PRIMARY KEY,
    pattern     TEXT NOT NULL,
    replacement TEXT NOT NULL,
    rule_type   TEXT NOT NULL DEFAULT 'Text',
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);
