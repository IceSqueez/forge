use super::reserved_keys as key;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingPortability {
    Portable,
    MachineLocal,
    RepositoryOwned,
}

use SettingPortability::{MachineLocal, Portable, RepositoryOwned};

pub const CLASSIFIED_KEYS: &[(&str, SettingPortability)] = &[
    (key::THEME, Portable),
    (key::ACCENT, Portable),
    (key::DENSITY, Portable),
    (key::FONT_BODY, Portable),
    (key::FONT_MONO, Portable),
    (key::EVENT_LOG_RETENTION_DAYS, Portable),
    (key::ACTION_HISTORY_RETENTION_DAYS, Portable),
    (key::SERVER_ENABLED, Portable),
    (key::SERVER_BIND_ADDRESS, MachineLocal),
    (key::SERVER_PORT, Portable),
    (key::SERVER_LAN_BIND_ENABLED, MachineLocal),
    (key::SERVER_AUTH_REQUIRED_FOR_READS, MachineLocal),
    (key::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN, MachineLocal),
    (key::SERVER_OVERLAY_CORS_ANY_ORIGIN, MachineLocal),
    (key::SERVER_OVERLAY_ROOT, MachineLocal),
    (key::SERVER_ADDITIONAL_ORIGINS, MachineLocal),
    (key::SCRIPT_HTTP_ALLOWED_DOMAINS, MachineLocal),
    (key::SCRIPT_HTTP_MAX_CALLS, Portable),
    (key::SCRIPT_HTTP_TIMEOUT_MS, Portable),
    (key::SCRIPT_HTTP_ALLOW_LOCAL, MachineLocal),
    (key::SCRIPT_HTTP_MAX_RESPONSE_BYTES, Portable),
    (key::CORE_HTTP_ALLOW_LOCAL, MachineLocal),
    (key::SCRIPT_OP_LIMIT, Portable),
    (key::SCRIPT_TIMEOUT_MS, Portable),
    (key::LANGUAGE, Portable),
    (key::REGION, Portable),
    (key::DATE_FORMAT, Portable),
    (key::TIME_FORMAT, Portable),
    (key::FIRST_DAY_OF_WEEK, Portable),
    (key::KEYBOARD_SHORTCUTS, Portable),
    (key::AUDIO_OUTPUT_DEVICE_ID, MachineLocal),
    (key::AUDIO_VOICE_GATE_ENABLED, Portable),
    (key::AUDIO_VOICE_GATE_INPUT_DEVICE_ID, MachineLocal),
    (key::AUDIO_VOICE_GATE_THRESHOLD, Portable),
    (key::AUDIO_VOICE_GATE_HOLD_MS, Portable),
    (key::AUDIO_SPEECH_ROUTE, Portable),
    (key::AUDIO_CLIPS_ROUTE, Portable),
    (key::AUDIO_OVERLAY_ID, Portable),
    (key::CHAT_HISTORY_PER_VIEWER_LIMIT, Portable),
    (key::CHAT_HISTORY_DISPLAY_LIMIT, Portable),
    (key::PICKER_FAVORITES_SUB_ACTIONS, Portable),
    (key::PICKER_FAVORITES_TRIGGERS, Portable),
    (key::PICKER_FAVORITES_ICONS, Portable),
    (key::TTS_DISABLED_ENGINES, Portable),
    (key::TTS_SYNTHESIS_DEFAULTS, Portable),
    (key::TTS_MASTER_VOLUME, Portable),
    (key::SOUNDBOARD_ENABLED, Portable),
    (key::SOUNDBOARD_OUTPUT_DEVICE, MachineLocal),
    (key::SOUNDBOARD_MASTER_VOLUME, Portable),
    (key::SOUNDBOARD_ALSO_HEADPHONES, Portable),
    (key::DIAGNOSTICS_LOG_LEVEL, Portable),
    (key::STAY_AWAKE, MachineLocal),
    (key::WINDOW_PLACEMENT, MachineLocal),
    (key::UPDATES_ROLLED_BACK_VERSION, MachineLocal),
    (key::UPDATES_LEFTOVER_INSTALL_NOTICED, MachineLocal),
    (key::CREDENTIALS_KEY_LOSS, MachineLocal),
    (key::RETIRED_MIDI_ENABLED, MachineLocal),
    (key::RETIRED_HOTKEY_ENABLED, MachineLocal),
    (key::OBS_AUTO_RECONNECT, Portable),
    (key::OBS_CONNECT_ON_LAUNCH, Portable),
    (key::VTUBE_AUTO_RECONNECT, Portable),
    (key::VTUBE_CONNECT_ON_LAUNCH, Portable),
    (key::MIDI_KNOWN_DEVICES, MachineLocal),
    (key::HOTKEY_HOLD_CEILING_SECS, Portable),
    (key::DONATELLO_POLL_INTERVAL_SECS, Portable),
    (key::VOICE_ASSIGNMENT_STRATEGY, RepositoryOwned),
    (key::NOTIFICATIONS_ACTION_FAILURE_OS_NOTICE, Portable),
    (key::NOTIFICATIONS_CONNECTION_LOSS_SOUND, Portable),
    (
        key::NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE,
        Portable,
    ),
    (key::BACKUP_SCHEDULE_ENABLED, MachineLocal),
    (key::BACKUP_SCHEDULE_INTERVAL_MINUTES, Portable),
    (key::BACKUP_SCHEDULE_KEEP_NEWEST, Portable),
    (key::BACKUP_SCHEDULE_KEEP_DAILY_DAYS, Portable),
    (key::BACKUP_SCHEDULE_INCLUDE_MEDIA, Portable),
    (key::BACKUP_SCHEDULE_INCLUDE_VOICES, Portable),
    (key::BACKUP_SCHEDULE_INCLUDE_OVERLAYS, Portable),
    (key::BACKUP_SCHEDULE_INCLUDE_ASSETS, Portable),
    (key::BACKUP_SCHEDULE_DESTINATION, MachineLocal),
    (key::BACKUP_SCHEDULE_KEY, MachineLocal),
    (key::BACKUP_SCHEDULE_LAST_SUCCESS_AT, MachineLocal),
    (key::BACKUP_SCHEDULE_CONTENT_FINGERPRINT, MachineLocal),
    (key::SAFETY_COPIES_MODE, MachineLocal),
    (key::SAFETY_COPIES_KEEP_NEWEST, MachineLocal),
    (key::SAFETY_COPIES_DELETE_AFTER_DAYS, MachineLocal),
];

pub const CLASSIFIED_PREFIXES: &[(&str, SettingPortability)] = &[
    (key::TTS_ENGINE_PARAMS_PREFIX, Portable),
    (key::INTEGRATION_ENABLED_PREFIX, Portable),
];

pub fn portability_of(stored_key: &str) -> SettingPortability {
    CLASSIFIED_KEYS
        .iter()
        .find(|(classified, _)| *classified == stored_key)
        .or_else(|| {
            CLASSIFIED_PREFIXES
                .iter()
                .find(|(prefix, _)| stored_key.starts_with(prefix))
        })
        .map_or(MachineLocal, |(_, portability)| *portability)
}

pub fn is_portable(stored_key: &str) -> bool {
    portability_of(stored_key) == Portable
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    const SETTINGS_SOURCE: &str = include_str!("../settings.rs");

    fn reserved_key_values() -> Vec<String> {
        let block = SETTINGS_SOURCE
            .split_once("pub mod reserved_keys {")
            .and_then(|(_, rest)| rest.split_once("\n}"))
            .map(|(block, _)| block)
            .unwrap_or_default();
        block
            .lines()
            .filter_map(|line| line.trim().strip_prefix("pub const "))
            .filter_map(|declaration| declaration.split_once("= \""))
            .filter_map(|(_, value)| value.split_once("\";"))
            .map(|(value, _)| value.to_owned())
            .collect()
    }

    fn classified() -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for (key, _) in CLASSIFIED_KEYS.iter().chain(CLASSIFIED_PREFIXES) {
            *counts.entry(*key).or_insert(0) += 1;
        }
        counts
    }

    #[test]
    fn every_reserved_key_is_classified_exactly_once() {
        let reserved = reserved_key_values();
        let classified = classified();

        let unclassified: Vec<_> = reserved
            .iter()
            .filter(|key| !classified.contains_key(key.as_str()))
            .collect();
        let doubled: Vec<_> = classified.iter().filter(|(_, n)| **n > 1).collect();

        assert!(reserved.len() > 50, "parsed only {reserved:?}");
        assert_eq!((unclassified, doubled), (Vec::new(), Vec::new()));
    }

    #[test]
    fn a_key_outside_the_catalog_is_machine_local() {
        for key in [
            "ui.last_note",
            "",
            "THEME",
            "theme ",
            " theme",
            "tts.engine_params",
            "integration.enabled",
            "collections.twitch.rewards",
        ] {
            assert_eq!(portability_of(key), MachineLocal, "{key:?}");
        }
    }

    #[test]
    fn a_key_under_a_portable_prefix_is_portable() {
        for key in [
            "tts.engine_params.piper",
            "tts.engine_params.",
            "integration.enabled.twitch",
        ] {
            assert!(is_portable(key), "{key:?}");
        }
    }

    #[test]
    fn window_devices_paths_update_state_and_stay_awake_never_travel() {
        for key in [
            key::WINDOW_PLACEMENT,
            key::AUDIO_OUTPUT_DEVICE_ID,
            key::AUDIO_VOICE_GATE_INPUT_DEVICE_ID,
            key::SOUNDBOARD_OUTPUT_DEVICE,
            key::MIDI_KNOWN_DEVICES,
            key::SERVER_OVERLAY_ROOT,
            key::UPDATES_ROLLED_BACK_VERSION,
            key::UPDATES_LEFTOVER_INSTALL_NOTICED,
            key::CREDENTIALS_KEY_LOSS,
            key::STAY_AWAKE,
        ] {
            assert_eq!(portability_of(key), MachineLocal, "{key}");
        }
    }

    #[test]
    fn network_exposure_and_http_trust_settings_never_travel() {
        for key in [
            key::SERVER_BIND_ADDRESS,
            key::SERVER_LAN_BIND_ENABLED,
            key::SERVER_AUTH_REQUIRED_FOR_READS,
            key::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN,
            key::SERVER_OVERLAY_CORS_ANY_ORIGIN,
            key::SERVER_ADDITIONAL_ORIGINS,
            key::SCRIPT_HTTP_ALLOWED_DOMAINS,
            key::SCRIPT_HTTP_ALLOW_LOCAL,
            key::CORE_HTTP_ALLOW_LOCAL,
        ] {
            assert_eq!(portability_of(key), MachineLocal, "{key}");
        }
    }

    #[test]
    fn the_backup_schedule_key_and_its_machine_state_never_travel() {
        for key in [
            key::BACKUP_SCHEDULE_KEY,
            key::BACKUP_SCHEDULE_ENABLED,
            key::BACKUP_SCHEDULE_DESTINATION,
            key::BACKUP_SCHEDULE_LAST_SUCCESS_AT,
            key::BACKUP_SCHEDULE_CONTENT_FINGERPRINT,
        ] {
            assert_eq!(portability_of(key), MachineLocal, "{key}");
        }
    }

    #[test]
    fn the_whole_appearance_and_region_format_travel_with_a_backup() {
        for key in [
            key::THEME,
            key::ACCENT,
            key::DENSITY,
            key::FONT_BODY,
            key::FONT_MONO,
            key::REGION,
            key::DATE_FORMAT,
            key::TIME_FORMAT,
            key::FIRST_DAY_OF_WEEK,
        ] {
            assert!(is_portable(key), "{key}");
        }
    }

    #[test]
    fn the_voice_strategy_row_is_left_to_its_repository() {
        assert!(!is_portable(key::VOICE_ASSIGNMENT_STRATEGY));
    }
}
