UPDATE overlays
SET kind_id = 'overlay.blank',
    config_schema_version = 1,
    config = json_remove(
        config,
        '$.clip_id',
        '$.clip_path',
        '$.report_path',
        '$.clip_media_type',
        '$.clip_duration_ms',
        '$.command'
    ),
    retained_content = NULL
WHERE kind_id = 'overlay.audio';
