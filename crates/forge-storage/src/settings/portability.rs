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
    (key::DENSITY, Portable),
    (key::FONT_BODY, Portable),
    (key::FONT_MONO, Portable),
    (key::EVENT_LOG_RETENTION_DAYS, Portable),
    (key::ACTION_HISTORY_RETENTION_DAYS, Portable),
    (key::SERVER_ENABLED, Portable),
    (key::SERVER_BIND_ADDRESS, Portable),
    (key::SERVER_PORT, Portable),
    (key::SERVER_LAN_BIND_ENABLED, Portable),
    (key::SERVER_AUTH_REQUIRED_FOR_READS, Portable),
    (key::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN, Portable),
    (key::SERVER_OVERLAY_CORS_ANY_ORIGIN, Portable),
    (key::SERVER_OVERLAY_ROOT, MachineLocal),
    (key::SERVER_ADDITIONAL_ORIGINS, Portable),
    (key::SCRIPT_HTTP_ALLOWED_DOMAINS, Portable),
    (key::SCRIPT_HTTP_MAX_CALLS, Portable),
    (key::SCRIPT_HTTP_TIMEOUT_MS, Portable),
    (key::SCRIPT_HTTP_ALLOW_LOCAL, Portable),
    (key::SCRIPT_HTTP_MAX_RESPONSE_BYTES, Portable),
    (key::CORE_HTTP_ALLOW_LOCAL, Portable),
    (key::SCRIPT_OP_LIMIT, Portable),
    (key::SCRIPT_TIMEOUT_MS, Portable),
    (key::LANGUAGE, Portable),
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
