ALTER TABLE tts_pipeline_settings ADD COLUMN skip_prefix TEXT;
ALTER TABLE tts_pipeline_settings ADD COLUMN skip_emote_only INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tts_pipeline_settings ADD COLUMN skip_mostly_non_latin INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tts_pipeline_settings ADD COLUMN skip_custom_regexes TEXT NOT NULL DEFAULT '[]';
ALTER TABLE tts_pipeline_settings ADD COLUMN output_sanitize_punctuation INTEGER NOT NULL DEFAULT 0;
