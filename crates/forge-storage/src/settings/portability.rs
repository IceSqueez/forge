#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingPortability {
    Portable,
    MachineLocal,
    RepositoryOwned,
}

#[cfg(test)]
mod tests {
    use super::super::reserved_keys as key;
    use super::SettingPortability::MachineLocal;
    use super::*;

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
