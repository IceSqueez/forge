UPDATE tts_pipeline_settings
SET skip_longer_than = 1,
    longer_than_max_chars = max_length
WHERE max_length IS NOT NULL AND max_length > 0;

ALTER TABLE tts_pipeline_settings DROP COLUMN max_length;
