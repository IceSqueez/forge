CREATE TABLE globals (
    name          TEXT    PRIMARY KEY,
    value         TEXT    NOT NULL,
    type_tag      TEXT    NOT NULL,
    persisted     INTEGER NOT NULL DEFAULT 1,
    reads         INTEGER NOT NULL DEFAULT 0,
    writes        INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    last_modified INTEGER NOT NULL,
    archived_at   INTEGER
);
CREATE INDEX globals_persisted ON globals(persisted);
CREATE INDEX globals_archived ON globals(archived_at) WHERE archived_at IS NOT NULL;

CREATE TABLE user_globals (
    broadcaster_id TEXT    NOT NULL,
    user_id        TEXT    NOT NULL,
    name           TEXT    NOT NULL,
    value          TEXT    NOT NULL,
    type_tag       TEXT    NOT NULL,
    last_modified  INTEGER NOT NULL,
    PRIMARY KEY (broadcaster_id, user_id, name)
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE action_history (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    action_id           TEXT    NOT NULL,
    triggering_event_id TEXT,
    started_at          INTEGER NOT NULL,
    duration_ms         INTEGER NOT NULL,
    outcome             TEXT    NOT NULL,
    context             TEXT    NOT NULL
);
CREATE INDEX action_history_by_action ON action_history(action_id, started_at DESC);
CREATE INDEX action_history_by_event ON action_history(triggering_event_id) WHERE triggering_event_id IS NOT NULL;

CREATE TABLE credentials (
    id           TEXT PRIMARY KEY,
    encrypted    BLOB NOT NULL,
    nonce        BLOB NOT NULL,
    last_refresh INTEGER NOT NULL
);

CREATE TABLE queues (
    id          TEXT    PRIMARY KEY NOT NULL,
    name        TEXT    NOT NULL,
    blocking    INTEGER NOT NULL DEFAULT 0,
    description TEXT    NOT NULL DEFAULT '',
    concurrency INTEGER NOT NULL DEFAULT 8,
    paused      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE actions (
    id             TEXT    PRIMARY KEY NOT NULL,
    name           TEXT    NOT NULL,
    group_name     TEXT    NOT NULL DEFAULT '',
    queue_id       TEXT    NOT NULL REFERENCES queues(id),
    enabled        INTEGER NOT NULL DEFAULT 1,
    concurrent     INTEGER NOT NULL DEFAULT 0,
    bypass_pause   INTEGER NOT NULL DEFAULT 0,
    description    TEXT    NOT NULL DEFAULT '',
    sub_actions    TEXT    NOT NULL DEFAULT '[]',
    execution_mode TEXT    NOT NULL DEFAULT 'sequential',
    format_version INTEGER NOT NULL DEFAULT 0,
    archived_at    INTEGER
);
CREATE INDEX idx_actions_group ON actions(group_name);
CREATE INDEX idx_actions_queue ON actions(queue_id);
CREATE INDEX actions_archived ON actions(archived_at) WHERE archived_at IS NOT NULL;

CREATE TABLE scripts (
    id            TEXT PRIMARY KEY,
    name          TEXT UNIQUE NOT NULL,
    body          TEXT NOT NULL,
    description   TEXT,
    enabled       INTEGER NOT NULL,
    created_at    INTEGER NOT NULL,
    last_modified INTEGER NOT NULL,
    contract_json TEXT NOT NULL DEFAULT '{}',
    body_hash     TEXT NOT NULL DEFAULT ''
);
CREATE INDEX scripts_enabled ON scripts(enabled);

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

CREATE TABLE soundboard_clips (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    file_path     TEXT NOT NULL,
    volume        REAL NOT NULL DEFAULT 1.0,
    output_device TEXT NOT NULL,
    hotkey        TEXT,
    created_at    INTEGER NOT NULL,
    category      TEXT NOT NULL DEFAULT '',
    loop_playback INTEGER NOT NULL DEFAULT 0,
    duration_secs REAL,
    builtin_id    TEXT
);
CREATE INDEX idx_soundboard_clips_name ON soundboard_clips(name);
CREATE INDEX idx_soundboard_clips_category ON soundboard_clips(category);

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
    id                 INTEGER PRIMARY KEY CHECK (id = 1),
    excluded_voice_ids TEXT NOT NULL DEFAULT '[]',
    excluded_locales   TEXT NOT NULL DEFAULT '[]',
    updated_at         TEXT NOT NULL
);

CREATE TABLE replacement_rules (
    id          TEXT PRIMARY KEY,
    pattern     TEXT NOT NULL,
    replacement TEXT NOT NULL,
    rule_type   TEXT NOT NULL DEFAULT 'Text',
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

CREATE TABLE viewers (
    platform        TEXT    NOT NULL,
    viewer_id       TEXT    NOT NULL,
    username        TEXT    NOT NULL,
    first_seen_at   INTEGER NOT NULL,
    last_seen_at    INTEGER NOT NULL,
    message_count   INTEGER NOT NULL DEFAULT 0,
    custom_greeting INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (platform, viewer_id)
);
CREATE INDEX viewers_last_seen ON viewers(last_seen_at DESC);
CREATE INDEX viewers_by_username ON viewers(username COLLATE NOCASE);

CREATE TABLE action_executions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    action_id     TEXT NOT NULL,
    started_at    INTEGER NOT NULL,
    duration_ms   INTEGER NOT NULL,
    status        TEXT NOT NULL CHECK (status IN ('ok','err')),
    error_message TEXT
);
CREATE INDEX idx_action_executions_action_started ON action_executions(action_id, started_at DESC);
CREATE INDEX idx_action_executions_started ON action_executions(started_at);

CREATE TABLE trigger_instances (
    id              TEXT PRIMARY KEY,
    kind_id         TEXT NOT NULL,
    name            TEXT NOT NULL,
    overrides       TEXT NOT NULL DEFAULT '{}',
    enabled         INTEGER NOT NULL DEFAULT 1,
    user_defined    INTEGER NOT NULL DEFAULT 0,
    platform_scope  TEXT NOT NULL DEFAULT '"any"',
    archived_at     INTEGER,
    cooldown_secs   INTEGER NOT NULL DEFAULT 0,
    cooldown_global INTEGER NOT NULL DEFAULT 1,
    permission_rung TEXT NOT NULL DEFAULT 'everyone'
);
CREATE UNIQUE INDEX idx_trigger_instances_default_unique ON trigger_instances(kind_id) WHERE user_defined = 0;
CREATE INDEX idx_trigger_instances_user_defined ON trigger_instances(user_defined);
CREATE INDEX trigger_instances_archived ON trigger_instances(archived_at) WHERE archived_at IS NOT NULL;

CREATE TABLE action_trigger_instances (
    action_id           TEXT NOT NULL,
    trigger_instance_id TEXT NOT NULL,
    position            INTEGER NOT NULL,
    PRIMARY KEY (action_id, trigger_instance_id),
    FOREIGN KEY (action_id) REFERENCES actions(id) ON DELETE CASCADE,
    FOREIGN KEY (trigger_instance_id) REFERENCES trigger_instances(id) ON DELETE RESTRICT
);
CREATE INDEX idx_action_trigger_instances_instance ON action_trigger_instances(trigger_instance_id);

CREATE TABLE tts_filter_rules (
    id       TEXT PRIMARY KEY,
    name     TEXT NOT NULL DEFAULT '',
    enabled  INTEGER NOT NULL DEFAULT 1,
    position INTEGER NOT NULL,
    kind     TEXT NOT NULL,
    params   TEXT NOT NULL
);
CREATE INDEX ix_tts_filter_rules_position ON tts_filter_rules(position ASC);

CREATE TABLE tts_pipeline_settings (
    id                             INTEGER PRIMARY KEY CHECK (id = 1),
    url_mode                       TEXT NOT NULL DEFAULT 'speak',
    max_length                     INTEGER,
    blocklist_mode                 TEXT NOT NULL DEFAULT 'censor',
    strip_twitch_emotes            INTEGER NOT NULL DEFAULT 1,
    strip_reward_emotes            INTEGER NOT NULL DEFAULT 1,
    skip_contains_url              INTEGER NOT NULL DEFAULT 0,
    skip_starts_with_bang          INTEGER NOT NULL DEFAULT 0,
    skip_from_bot_accounts         INTEGER NOT NULL DEFAULT 0,
    bot_accounts                   TEXT NOT NULL DEFAULT '[]',
    skip_longer_than               INTEGER NOT NULL DEFAULT 0,
    longer_than_max_chars          INTEGER NOT NULL DEFAULT 200,
    skip_repeat_of_recent          INTEGER NOT NULL DEFAULT 0,
    repeat_of_recent_window        INTEGER NOT NULL DEFAULT 3,
    output_read_display_name_first INTEGER NOT NULL DEFAULT 0,
    output_emote_to_word           INTEGER NOT NULL DEFAULT 0,
    skip_prefix                    TEXT,
    skip_emote_only                INTEGER NOT NULL DEFAULT 0,
    skip_mostly_non_latin          INTEGER NOT NULL DEFAULT 0,
    skip_custom_regexes            TEXT NOT NULL DEFAULT '[]',
    output_sanitize_punctuation    INTEGER NOT NULL DEFAULT 0,
    output_max_duration_secs       INTEGER,
    output_language_aware_voice    INTEGER
);

CREATE TABLE chat_history (
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
CREATE INDEX idx_chat_history_received_at ON chat_history (received_at);
CREATE INDEX idx_chat_history_source_author ON chat_history (source, author);

CREATE TABLE script_executions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    script_id   TEXT NOT NULL,
    started_at  INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('ok','err'))
);
CREATE INDEX idx_script_executions_script_started ON script_executions(script_id, started_at DESC);
CREATE INDEX idx_script_executions_started ON script_executions(started_at);

CREATE TABLE overlays (
    id                    TEXT PRIMARY KEY NOT NULL,
    display_name          TEXT NOT NULL,
    kind_id               TEXT NOT NULL,
    enabled               INTEGER NOT NULL DEFAULT 1,
    position              INTEGER NOT NULL DEFAULT 0,
    config                TEXT NOT NULL DEFAULT '{}',
    config_schema_version INTEGER NOT NULL DEFAULT 0,
    generator_version     INTEGER NOT NULL DEFAULT 0,
    source_overrides      TEXT NOT NULL DEFAULT '[]',
    credential            TEXT NOT NULL,
    created_at            INTEGER NOT NULL,
    updated_at            INTEGER NOT NULL,
    retained_content      TEXT
);
CREATE INDEX idx_overlays_position ON overlays(position);
CREATE UNIQUE INDEX idx_overlays_credential ON overlays(credential);

CREATE TABLE media_blobs (
    id          TEXT PRIMARY KEY NOT NULL,
    format      TEXT NOT NULL,
    byte_size   INTEGER NOT NULL,
    label       TEXT NOT NULL,
    imported_at INTEGER NOT NULL
);

CREATE TABLE media_references (
    referrer_kind TEXT NOT NULL,
    referrer_id   TEXT NOT NULL,
    slot          TEXT NOT NULL,
    blob_id       TEXT NOT NULL REFERENCES media_blobs(id) ON DELETE RESTRICT,
    PRIMARY KEY (referrer_kind, referrer_id, slot)
);
CREATE INDEX idx_media_references_blob ON media_references(blob_id);

INSERT INTO settings (key, value) VALUES ('app.language', 'en');

INSERT INTO queues (id, name, blocking, description, concurrency, paused)
VALUES ('00000000000000000000000000', 'Default', 0, 'Catch-all queue for actions without explicit queue assignment', 8, 0);

INSERT INTO ignore_profile (id, excluded_voice_ids, excluded_locales, updated_at)
VALUES (1, '[]', '[]', datetime('now'));

INSERT INTO tts_pipeline_settings (id, url_mode, max_length, blocklist_mode, strip_twitch_emotes, strip_reward_emotes)
VALUES (1, 'speak', NULL, 'censor', 1, 1);
