use super::disclosure::SettingDisclosure::{Kind, Presence, Verbatim, Withheld};
use super::portability::SettingPortability::{MachineLocal, Portable, RepositoryOwned};
use super::reserved_keys as key;
use super::spec::SettingEncoding::{Flag as FlagEncoding, Integer, Json, Text};
use super::spec::SettingSpec;
use super::typed::{
    Choice, Count, Flag, JsonDocument, Level, OptionalText, Setting, SettingFamily, Tally,
    Verbosity,
};
use super::{
    DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT, DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT,
    DEFAULT_DIAGNOSTIC_LOG_LEVEL, EngineParams, FULL_LEVEL, MAX_CHAT_HISTORY_DISPLAY_LIMIT,
    MAX_CHAT_HISTORY_PER_VIEWER_LIMIT, MAX_VOICE_GATE_HOLD_MS, MIN_CHAT_HISTORY_DISPLAY_LIMIT,
    MIN_CHAT_HISTORY_PER_VIEWER_LIMIT, MIN_VOICE_GATE_HOLD_MS, SILENT_LEVEL,
    UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT, VOICE_GATE_DEFAULT_HOLD_MS,
    VOICE_GATE_DEFAULT_THRESHOLD,
};
use forge_speech_types::{AssignmentStrategy, SynthesisDefaults};

use crate::event_log::{
    DEFAULT_EVENT_LOG_RETENTION_DAYS, MAX_EVENT_LOG_RETENTION_DAYS, MIN_EVENT_LOG_RETENTION_DAYS,
};
use crate::history::{MAX_ACTION_HISTORY_RETENTION_DAYS, MIN_ACTION_HISTORY_RETENTION_DAYS};

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

pub const ACTION_HISTORY_RETENTION_DAYS: Setting<Count> = Setting::new(
    key::ACTION_HISTORY_RETENTION_DAYS,
    Count {
        default: DEFAULT_EVENT_LOG_RETENTION_DAYS,
        min: MIN_ACTION_HISTORY_RETENTION_DAYS,
        max: MAX_ACTION_HISTORY_RETENTION_DAYS,
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

pub const DIAGNOSTICS_LOG_LEVEL: Setting<Verbosity> = Setting::new(
    key::DIAGNOSTICS_LOG_LEVEL,
    Verbosity {
        default: DEFAULT_DIAGNOSTIC_LOG_LEVEL,
    },
    Portable,
    Verbatim,
);

const LANGUAGE_CODES: &[&str] = &["en", "uk"];
const DEFAULT_LANGUAGE_CODE: &str = "en";

pub const LANGUAGE: Setting<Choice> = Setting::new(
    key::LANGUAGE,
    Choice {
        default: DEFAULT_LANGUAGE_CODE,
        options: LANGUAGE_CODES,
    },
    Portable,
    Verbatim,
);

pub const REGION: Setting<OptionalText> =
    Setting::new(key::REGION, OptionalText, Portable, Verbatim);

pub const DATE_FORMAT: Setting<OptionalText> =
    Setting::new(key::DATE_FORMAT, OptionalText, Portable, Verbatim);

pub const TIME_FORMAT: Setting<OptionalText> =
    Setting::new(key::TIME_FORMAT, OptionalText, Portable, Verbatim);

pub const FIRST_DAY_OF_WEEK: Setting<OptionalText> =
    Setting::new(key::FIRST_DAY_OF_WEEK, OptionalText, Portable, Verbatim);

pub const THEME: Setting<OptionalText> = Setting::new(key::THEME, OptionalText, Portable, Verbatim);

pub const ACCENT: Setting<OptionalText> =
    Setting::new(key::ACCENT, OptionalText, Portable, Verbatim);

pub const AUDIO_OVERLAY_ID: Setting<OptionalText> =
    Setting::new(key::AUDIO_OVERLAY_ID, OptionalText, Portable, Presence);

pub const FONT_BODY: Setting<OptionalText> =
    Setting::new(key::FONT_BODY, OptionalText, Portable, Presence);

pub const FONT_MONO: Setting<OptionalText> =
    Setting::new(key::FONT_MONO, OptionalText, Portable, Presence);

pub const AUDIO_OUTPUT_DEVICE_ID: Setting<OptionalText> = Setting::new(
    key::AUDIO_OUTPUT_DEVICE_ID,
    OptionalText,
    MachineLocal,
    Presence,
);

pub const AUDIO_VOICE_GATE_INPUT_DEVICE_ID: Setting<OptionalText> = Setting::new(
    key::AUDIO_VOICE_GATE_INPUT_DEVICE_ID,
    OptionalText,
    MachineLocal,
    Presence,
);

pub const SERVER_OVERLAY_ROOT: Setting<OptionalText> = Setting::new(
    key::SERVER_OVERLAY_ROOT,
    OptionalText,
    MachineLocal,
    Presence,
);

pub const TTS_DISABLED_ENGINES: Setting<JsonDocument<Vec<String>>> = Setting::new(
    key::TTS_DISABLED_ENGINES,
    JsonDocument::new(),
    Portable,
    Verbatim,
);

pub const TTS_SYNTHESIS_DEFAULTS: Setting<JsonDocument<SynthesisDefaults>> = Setting::new(
    key::TTS_SYNTHESIS_DEFAULTS,
    JsonDocument::new(),
    Portable,
    Verbatim,
);

pub const VOICE_ASSIGNMENT_STRATEGY: Setting<JsonDocument<AssignmentStrategy>> = Setting::new(
    key::VOICE_ASSIGNMENT_STRATEGY,
    JsonDocument::new(),
    RepositoryOwned,
    Withheld,
);

pub const CREDENTIALS_KEY_LOSS: Setting<Tally> = Setting::new(
    key::CREDENTIALS_KEY_LOSS,
    Tally { default: 0 },
    MachineLocal,
    Verbatim,
);

pub const TTS_ENGINE_PARAMS: SettingFamily<JsonDocument<EngineParams>> = SettingFamily::new(
    key::TTS_ENGINE_PARAMS_PREFIX,
    JsonDocument::new(),
    Portable,
    Verbatim,
);

pub const INTEGRATION_ENABLED: SettingFamily<Flag> = SettingFamily::new(
    key::INTEGRATION_ENABLED_PREFIX,
    Flag { default: false },
    Portable,
    Verbatim,
);

pub const CORE_SETTINGS: &[SettingSpec] = &[
    THEME.spec(),
    ACCENT.spec(),
    FONT_BODY.spec(),
    FONT_MONO.spec(),
    EVENT_LOG_RETENTION_DAYS.spec(),
    ACTION_HISTORY_RETENTION_DAYS.spec(),
    LANGUAGE.spec(),
    REGION.spec(),
    DATE_FORMAT.spec(),
    TIME_FORMAT.spec(),
    FIRST_DAY_OF_WEEK.spec(),
    AUDIO_OUTPUT_DEVICE_ID.spec(),
    AUDIO_VOICE_GATE_ENABLED.spec(),
    AUDIO_VOICE_GATE_INPUT_DEVICE_ID.spec(),
    AUDIO_VOICE_GATE_THRESHOLD.spec(),
    AUDIO_VOICE_GATE_HOLD_MS.spec(),
    AUDIO_OVERLAY_ID.spec(),
    CHAT_HISTORY_PER_VIEWER_LIMIT.spec(),
    CHAT_HISTORY_DISPLAY_LIMIT.spec(),
    SettingSpec::exact(key::PICKER_FAVORITES_SUB_ACTIONS, Json, Portable, Kind),
    SettingSpec::exact(key::PICKER_FAVORITES_TRIGGERS, Json, Portable, Kind),
    SettingSpec::exact(key::PICKER_FAVORITES_ICONS, Json, Portable, Kind),
    TTS_DISABLED_ENGINES.spec(),
    TTS_SYNTHESIS_DEFAULTS.spec(),
    TTS_MASTER_VOLUME.spec(),
    TTS_ENGINE_PARAMS.spec(),
    DIAGNOSTICS_LOG_LEVEL.spec(),
    CREDENTIALS_KEY_LOSS.spec(),
    SERVER_OVERLAY_ROOT.spec(),
    INTEGRATION_ENABLED.spec(),
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
    VOICE_ASSIGNMENT_STRATEGY.spec(),
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
