-- Singleton TTS trigger-source settings row (id = 1, always present after migration).
CREATE TABLE tts_trigger_settings (
    id                     INTEGER PRIMARY KEY CHECK (id = 1),
    command_enabled        INTEGER NOT NULL DEFAULT 1,
    channel_points_enabled INTEGER NOT NULL DEFAULT 1,
    bits_enabled           INTEGER NOT NULL DEFAULT 1,
    sub_messages_enabled   INTEGER NOT NULL DEFAULT 0,
    read_username          INTEGER NOT NULL DEFAULT 1,
    speak_emotes           INTEGER NOT NULL DEFAULT 0,
    bits_skip_line         INTEGER NOT NULL DEFAULT 1
);

INSERT OR IGNORE INTO tts_trigger_settings
    (id, command_enabled, channel_points_enabled, bits_enabled, sub_messages_enabled,
     read_username, speak_emotes, bits_skip_line)
VALUES
    (1, 1, 1, 1, 0, 1, 0, 1);
