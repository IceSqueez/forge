ALTER TABLE settings ADD COLUMN portability TEXT NOT NULL DEFAULT 'machine_local'
    CHECK (portability IN ('portable', 'machine_local', 'repository_owned'));

ALTER TABLE settings ADD COLUMN disclosure TEXT NOT NULL DEFAULT 'withheld'
    CHECK (disclosure IN ('verbatim', 'kind', 'presence', 'withheld'));

UPDATE settings SET portability = 'portable', disclosure = 'verbatim'
 WHERE substr(key, 1, 20) = 'integration.enabled.';

UPDATE settings SET portability = 'portable', disclosure = 'verbatim'
 WHERE substr(key, 1, 18) = 'tts.engine_params.';

UPDATE settings SET portability = 'machine_local', disclosure = 'kind'
 WHERE key IN (
    'app.window_placement',
    'script.http_allowed_domains',
    'server.additional_origins'
);

UPDATE settings SET portability = 'machine_local', disclosure = 'presence'
 WHERE key IN (
    'audio.output_device_id',
    'audio.voice_gate_input_device_id',
    'server.bind_address',
    'server.overlay_root',
    'soundboard.output_device'
);

UPDATE settings SET portability = 'machine_local', disclosure = 'verbatim'
 WHERE key IN (
    'app.stay_awake',
    'core.http_allow_local',
    'credentials.key_loss_stranded',
    'hotkey.enabled',
    'integrations.first_run_completed',
    'midi.enabled',
    'script.http_allow_local',
    'server.auth_required_for_reads',
    'server.http_overlay_require_token',
    'server.lan_bind_enabled',
    'server.overlay_cors_any_origin',
    'updates.leftover_install_noticed',
    'updates.rolled_back_version'
);

UPDATE settings SET portability = 'machine_local', disclosure = 'withheld'
 WHERE key IN (
    'backup_schedule.content_fingerprint',
    'backup_schedule.destination',
    'backup_schedule.enabled',
    'backup_schedule.key',
    'backup_schedule.last_success_at',
    'midi.known_devices',
    'safety_copies.delete_after_days',
    'safety_copies.keep_newest',
    'safety_copies.mode'
);

UPDATE settings SET portability = 'portable', disclosure = 'kind'
 WHERE key IN (
    'app.keyboard_shortcuts',
    'picker.favorites.icons',
    'picker.favorites.sub_actions',
    'picker.favorites.triggers'
);

UPDATE settings SET portability = 'portable', disclosure = 'presence'
 WHERE key IN (
    'audio.overlay_id',
    'font_body',
    'font_mono'
);

UPDATE settings SET portability = 'portable', disclosure = 'verbatim'
 WHERE key IN (
    'accent',
    'action_history_retention_days',
    'app.date_format',
    'app.first_day_of_week',
    'app.language',
    'app.region',
    'app.time_format',
    'audio.clips_route',
    'audio.speech_route',
    'audio.voice_gate_enabled',
    'audio.voice_gate_hold_ms',
    'audio.voice_gate_threshold',
    'chat_history.display_limit',
    'chat_history.per_viewer_limit',
    'density',
    'diagnostics.log_level',
    'event_log_retention_days',
    'script.http_max_calls_per_script',
    'script.http_max_response_bytes',
    'script.http_timeout_ms',
    'script.op_limit',
    'script.timeout_ms',
    'server.enabled',
    'server.port',
    'soundboard.also_headphones',
    'soundboard.enabled',
    'soundboard.master_volume',
    'theme',
    'tts.disabled_engines',
    'tts.master_volume',
    'tts.synthesis_defaults'
);

UPDATE settings SET portability = 'portable', disclosure = 'withheld'
 WHERE key IN (
    'backup_schedule.include_assets',
    'backup_schedule.include_media',
    'backup_schedule.include_overlays',
    'backup_schedule.include_voices',
    'backup_schedule.interval_minutes',
    'backup_schedule.keep_daily_days',
    'backup_schedule.keep_newest',
    'donations.donatello.poll_interval_secs',
    'hotkey.hold_ceiling_secs',
    'notifications.action_failure_os_notice',
    'notifications.connection_loss_sound',
    'notifications.scheduled_backup_failure_os_notice',
    'obs.auto_reconnect',
    'obs.connect_on_launch',
    'vtube.auto_reconnect',
    'vtube.connect_on_launch'
);

UPDATE settings SET portability = 'repository_owned', disclosure = 'withheld'
 WHERE key IN (
    'voice:strategy'
);
