use std::sync::LazyLock;

use super::catalog::core_catalog;
use super::core_settings::CORE_SETTINGS;
use super::spec::SettingKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingPortability {
    Portable,
    MachineLocal,
    RepositoryOwned,
}

pub static CLASSIFIED_KEYS: LazyLock<Vec<(&'static str, SettingPortability)>> =
    LazyLock::new(|| core_classes(|key| matches!(key, SettingKey::Exact(_))));

pub static CLASSIFIED_PREFIXES: LazyLock<Vec<(&'static str, SettingPortability)>> =
    LazyLock::new(|| core_classes(|key| matches!(key, SettingKey::Prefix(_))));

fn core_classes(selected: fn(SettingKey) -> bool) -> Vec<(&'static str, SettingPortability)> {
    CORE_SETTINGS
        .iter()
        .filter(|spec| selected(spec.key))
        .map(|spec| (spec.key.name(), spec.portability))
        .collect()
}

pub fn portability_of(stored_key: &str) -> SettingPortability {
    core_catalog().portability_of(stored_key)
}

pub fn is_portable(stored_key: &str) -> bool {
    core_catalog().is_portable(stored_key)
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
