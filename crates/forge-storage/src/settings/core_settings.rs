use super::disclosure::SettingDisclosure::{Kind, Presence, Verbatim, Withheld};
use super::portability::SettingPortability::{MachineLocal, Portable, RepositoryOwned};
use super::reserved_keys as key;
use super::spec::SettingEncoding::{Flag as FlagEncoding, Integer, Json, Text};
use super::spec::SettingSpec;
use super::typed::{Count, Flag, Level, Setting, Verbosity};
use super::{
    DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT, DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT,
    DEFAULT_DIAGNOSTIC_LOG_LEVEL, FULL_LEVEL, MAX_CHAT_HISTORY_DISPLAY_LIMIT,
    MAX_CHAT_HISTORY_PER_VIEWER_LIMIT, MAX_VOICE_GATE_HOLD_MS, MIN_CHAT_HISTORY_DISPLAY_LIMIT,
    MIN_CHAT_HISTORY_PER_VIEWER_LIMIT, MIN_VOICE_GATE_HOLD_MS, SILENT_LEVEL,
    UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT, VOICE_GATE_DEFAULT_HOLD_MS,
    VOICE_GATE_DEFAULT_THRESHOLD,
};
use crate::event_log::{
    DEFAULT_EVENT_LOG_RETENTION_DAYS, MAX_EVENT_LOG_RETENTION_DAYS, MIN_EVENT_LOG_RETENTION_DAYS,
};

const FULL_SCALE_VOLUME: Level = Level {
    default: FULL_LEVEL,
    min: SILENT_LEVEL,
    max: FULL_LEVEL,
};

pub const EVENT_LOG_RETENTION_DAYS: Setting<Count> = Setting::new(
    key::EVENT_LOG_RETENTION_DAYS,
    Count {
        default: DEFAULT_EVENT_LOG_RETENTION_DAYS,
        min: MIN_EVENT_LOG_RETENTION_DAYS,
        max: MAX_EVENT_LOG_RETENTION_DAYS,
        unclamped: None,
    },
    Portable,
    Verbatim,
);

pub const AUDIO_VOICE_GATE_ENABLED: Setting<Flag> = Setting::new(
    key::AUDIO_VOICE_GATE_ENABLED,
    Flag { default: false },
    Portable,
    Verbatim,
);

pub const AUDIO_VOICE_GATE_THRESHOLD: Setting<Level> = Setting::new(
    key::AUDIO_VOICE_GATE_THRESHOLD,
    Level {
        default: VOICE_GATE_DEFAULT_THRESHOLD,
        min: SILENT_LEVEL,
        max: FULL_LEVEL,
    },
    Portable,
    Verbatim,
);

pub const AUDIO_VOICE_GATE_HOLD_MS: Setting<Count> = Setting::new(
    key::AUDIO_VOICE_GATE_HOLD_MS,
    Count {
        default: VOICE_GATE_DEFAULT_HOLD_MS,
        min: MIN_VOICE_GATE_HOLD_MS,
        max: MAX_VOICE_GATE_HOLD_MS,
        unclamped: None,
    },
    Portable,
    Verbatim,
);

pub const CHAT_HISTORY_PER_VIEWER_LIMIT: Setting<Count> = Setting::new(
    key::CHAT_HISTORY_PER_VIEWER_LIMIT,
    Count {
        default: DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT,
        min: MIN_CHAT_HISTORY_PER_VIEWER_LIMIT,
        max: MAX_CHAT_HISTORY_PER_VIEWER_LIMIT,
        unclamped: Some(UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT),
    },
    Portable,
    Verbatim,
);

pub const CHAT_HISTORY_DISPLAY_LIMIT: Setting<Count> = Setting::new(
    key::CHAT_HISTORY_DISPLAY_LIMIT,
    Count {
        default: DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT,
        min: MIN_CHAT_HISTORY_DISPLAY_LIMIT,
        max: MAX_CHAT_HISTORY_DISPLAY_LIMIT,
        unclamped: None,
    },
    Portable,
    Verbatim,
);

pub const TTS_MASTER_VOLUME: Setting<Level> = Setting::new(
    key::TTS_MASTER_VOLUME,
    FULL_SCALE_VOLUME,
    Portable,
    Verbatim,
);

pub const SOUNDBOARD_ENABLED: Setting<Flag> = Setting::new(
    key::SOUNDBOARD_ENABLED,
    Flag { default: true },
    Portable,
    Verbatim,
);

pub const SOUNDBOARD_MASTER_VOLUME: Setting<Level> = Setting::new(
    key::SOUNDBOARD_MASTER_VOLUME,
    FULL_SCALE_VOLUME,
    Portable,
    Verbatim,
);

pub const SOUNDBOARD_ALSO_HEADPHONES: Setting<Flag> = Setting::new(
    key::SOUNDBOARD_ALSO_HEADPHONES,
    Flag { default: false },
    Portable,
    Verbatim,
);

pub const DIAGNOSTICS_LOG_LEVEL: Setting<Verbosity> = Setting::new(
    key::DIAGNOSTICS_LOG_LEVEL,
    Verbosity {
        default: DEFAULT_DIAGNOSTIC_LOG_LEVEL,
    },
    Portable,
    Verbatim,
);

pub const CORE_SETTINGS: &[SettingSpec] = &[
    SettingSpec::exact(key::THEME, Text, Portable, Verbatim),
    SettingSpec::exact(key::ACCENT, Text, Portable, Verbatim),
    SettingSpec::exact(key::DENSITY, Text, Portable, Verbatim),
    SettingSpec::exact(key::FONT_BODY, Text, Portable, Presence),
    SettingSpec::exact(key::FONT_MONO, Text, Portable, Presence),
    EVENT_LOG_RETENTION_DAYS.spec(),
    SettingSpec::exact(
        key::ACTION_HISTORY_RETENTION_DAYS,
        Integer,
        Portable,
        Verbatim,
    ),
    SettingSpec::exact(key::SERVER_ENABLED, FlagEncoding, Portable, Verbatim),
    SettingSpec::exact(key::SERVER_BIND_ADDRESS, Text, MachineLocal, Presence),
    SettingSpec::exact(key::SERVER_PORT, Integer, Portable, Verbatim),
    SettingSpec::exact(
        key::SERVER_LAN_BIND_ENABLED,
        FlagEncoding,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(
        key::SERVER_AUTH_REQUIRED_FOR_READS,
        FlagEncoding,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(
        key::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN,
        FlagEncoding,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(
        key::SERVER_OVERLAY_CORS_ANY_ORIGIN,
        FlagEncoding,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(key::SERVER_OVERLAY_ROOT, Text, MachineLocal, Presence),
    SettingSpec::exact(key::SERVER_ADDITIONAL_ORIGINS, Json, MachineLocal, Kind),
    SettingSpec::exact(key::SCRIPT_HTTP_ALLOWED_DOMAINS, Json, MachineLocal, Kind),
    SettingSpec::exact(key::SCRIPT_HTTP_MAX_CALLS, Integer, Portable, Verbatim),
    SettingSpec::exact(key::SCRIPT_HTTP_TIMEOUT_MS, Integer, Portable, Verbatim),
    SettingSpec::exact(
        key::SCRIPT_HTTP_ALLOW_LOCAL,
        FlagEncoding,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(
        key::SCRIPT_HTTP_MAX_RESPONSE_BYTES,
        Integer,
        Portable,
        Verbatim,
    ),
    SettingSpec::exact(
        key::CORE_HTTP_ALLOW_LOCAL,
        FlagEncoding,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(key::SCRIPT_OP_LIMIT, Integer, Portable, Verbatim),
    SettingSpec::exact(key::SCRIPT_TIMEOUT_MS, Integer, Portable, Verbatim),
    SettingSpec::exact(key::LANGUAGE, Text, Portable, Verbatim),
    SettingSpec::exact(key::REGION, Text, Portable, Verbatim),
    SettingSpec::exact(key::DATE_FORMAT, Text, Portable, Verbatim),
    SettingSpec::exact(key::TIME_FORMAT, Text, Portable, Verbatim),
    SettingSpec::exact(key::FIRST_DAY_OF_WEEK, Text, Portable, Verbatim),
    SettingSpec::exact(key::KEYBOARD_SHORTCUTS, Json, Portable, Kind),
    SettingSpec::exact(key::AUDIO_OUTPUT_DEVICE_ID, Text, MachineLocal, Presence),
    AUDIO_VOICE_GATE_ENABLED.spec(),
    SettingSpec::exact(
        key::AUDIO_VOICE_GATE_INPUT_DEVICE_ID,
        Text,
        MachineLocal,
        Presence,
    ),
    AUDIO_VOICE_GATE_THRESHOLD.spec(),
    AUDIO_VOICE_GATE_HOLD_MS.spec(),
    SettingSpec::exact(key::AUDIO_SPEECH_ROUTE, Text, Portable, Verbatim),
    SettingSpec::exact(key::AUDIO_CLIPS_ROUTE, Text, Portable, Verbatim),
    SettingSpec::exact(key::AUDIO_OVERLAY_ID, Text, Portable, Presence),
    CHAT_HISTORY_PER_VIEWER_LIMIT.spec(),
    CHAT_HISTORY_DISPLAY_LIMIT.spec(),
    SettingSpec::exact(key::PICKER_FAVORITES_SUB_ACTIONS, Json, Portable, Kind),
    SettingSpec::exact(key::PICKER_FAVORITES_TRIGGERS, Json, Portable, Kind),
    SettingSpec::exact(key::PICKER_FAVORITES_ICONS, Json, Portable, Kind),
    SettingSpec::exact(key::TTS_DISABLED_ENGINES, Json, Portable, Verbatim),
    SettingSpec::exact(key::TTS_SYNTHESIS_DEFAULTS, Json, Portable, Verbatim),
    TTS_MASTER_VOLUME.spec(),
    SettingSpec::prefix(key::TTS_ENGINE_PARAMS_PREFIX, Json, Portable, Verbatim),
    SOUNDBOARD_ENABLED.spec(),
    SettingSpec::exact(key::SOUNDBOARD_OUTPUT_DEVICE, Text, MachineLocal, Presence),
    SOUNDBOARD_MASTER_VOLUME.spec(),
    SOUNDBOARD_ALSO_HEADPHONES.spec(),
    DIAGNOSTICS_LOG_LEVEL.spec(),
    SettingSpec::exact(key::STAY_AWAKE, FlagEncoding, MachineLocal, Verbatim),
    SettingSpec::exact(key::WINDOW_PLACEMENT, Json, MachineLocal, Kind),
    SettingSpec::exact(
        key::UPDATES_ROLLED_BACK_VERSION,
        Text,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(
        key::UPDATES_LEFTOVER_INSTALL_NOTICED,
        FlagEncoding,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(key::CREDENTIALS_KEY_LOSS, Integer, MachineLocal, Verbatim),
    SettingSpec::prefix(
        key::INTEGRATION_ENABLED_PREFIX,
        FlagEncoding,
        Portable,
        Verbatim,
    ),
    SettingSpec::exact(
        key::RETIRED_MIDI_ENABLED,
        FlagEncoding,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(
        key::RETIRED_HOTKEY_ENABLED,
        FlagEncoding,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(key::OBS_AUTO_RECONNECT, FlagEncoding, Portable, Withheld),
    SettingSpec::exact(key::OBS_CONNECT_ON_LAUNCH, FlagEncoding, Portable, Withheld),
    SettingSpec::exact(key::VTUBE_AUTO_RECONNECT, FlagEncoding, Portable, Withheld),
    SettingSpec::exact(
        key::VTUBE_CONNECT_ON_LAUNCH,
        FlagEncoding,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(key::MIDI_KNOWN_DEVICES, Json, MachineLocal, Withheld),
    SettingSpec::exact(key::HOTKEY_HOLD_CEILING_SECS, Integer, Portable, Withheld),
    SettingSpec::exact(
        key::DONATELLO_POLL_INTERVAL_SECS,
        Integer,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(
        key::VOICE_ASSIGNMENT_STRATEGY,
        Json,
        RepositoryOwned,
        Withheld,
    ),
    SettingSpec::exact(
        key::NOTIFICATIONS_ACTION_FAILURE_OS_NOTICE,
        FlagEncoding,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(
        key::NOTIFICATIONS_CONNECTION_LOSS_SOUND,
        FlagEncoding,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(
        key::NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE,
        FlagEncoding,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(
        key::BACKUP_SCHEDULE_ENABLED,
        FlagEncoding,
        MachineLocal,
        Withheld,
    ),
    SettingSpec::exact(
        key::BACKUP_SCHEDULE_INTERVAL_MINUTES,
        Integer,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(
        key::BACKUP_SCHEDULE_KEEP_NEWEST,
        Integer,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(
        key::BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
        Integer,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(
        key::BACKUP_SCHEDULE_INCLUDE_MEDIA,
        FlagEncoding,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(
        key::BACKUP_SCHEDULE_INCLUDE_VOICES,
        FlagEncoding,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(
        key::BACKUP_SCHEDULE_INCLUDE_OVERLAYS,
        FlagEncoding,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(
        key::BACKUP_SCHEDULE_INCLUDE_ASSETS,
        FlagEncoding,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(
        key::BACKUP_SCHEDULE_DESTINATION,
        Text,
        MachineLocal,
        Withheld,
    ),
    SettingSpec::exact(key::BACKUP_SCHEDULE_KEY, Json, MachineLocal, Withheld),
    SettingSpec::exact(
        key::BACKUP_SCHEDULE_LAST_SUCCESS_AT,
        Text,
        MachineLocal,
        Withheld,
    ),
    SettingSpec::exact(
        key::BACKUP_SCHEDULE_CONTENT_FINGERPRINT,
        Text,
        MachineLocal,
        Withheld,
    ),
    SettingSpec::exact(key::SAFETY_COPIES_MODE, Text, MachineLocal, Withheld),
    SettingSpec::exact(
        key::SAFETY_COPIES_KEEP_NEWEST,
        Integer,
        MachineLocal,
        Withheld,
    ),
    SettingSpec::exact(
        key::SAFETY_COPIES_DELETE_AFTER_DAYS,
        Integer,
        MachineLocal,
        Withheld,
    ),
];
