ALTER TABLE tts_pipeline_settings ADD COLUMN skip_prefix_enabled INTEGER NOT NULL DEFAULT 1;
ALTER TABLE tts_pipeline_settings ADD COLUMN output_max_duration_enabled INTEGER NOT NULL DEFAULT 1;

UPDATE tts_pipeline_settings
SET skip_custom_regexes = (
    SELECT json_group_array(json_object('pattern', pattern, 'enabled', json('true')))
    FROM (
        SELECT value AS pattern
        FROM json_each(tts_pipeline_settings.skip_custom_regexes)
        ORDER BY key
    )
);
