use super::disclosure::SettingDisclosure::{Kind, Presence, Verbatim, Withheld};
use super::portability::SettingPortability::{MachineLocal, Portable, RepositoryOwned};
use super::spec::SettingSpec;
use super::typed::{
    Choice, Count, Flag, JsonDocument, Level, OptionalText, Setting, SettingFamily, Tally,
    Verbosity,
};
use super::{
    DEFAULT_BACKUP_SCHEDULE_ENABLED, DEFAULT_BACKUP_SCHEDULE_INCLUDE_CATEGORY,
    DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES, DEFAULT_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
    DEFAULT_BACKUP_SCHEDULE_KEEP_NEWEST, DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT,
    DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT, DEFAULT_DIAGNOSTIC_LOG_LEVEL,
    DEFAULT_SAFETY_COPIES_DELETE_AFTER_DAYS, DEFAULT_SAFETY_COPIES_KEEP_NEWEST, EngineParams,
    FULL_LEVEL, MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES, MAX_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
    MAX_BACKUP_SCHEDULE_KEEP_NEWEST, MAX_CHAT_HISTORY_DISPLAY_LIMIT,
    MAX_CHAT_HISTORY_PER_VIEWER_LIMIT, MAX_SAFETY_COPIES_DELETE_AFTER_DAYS,
    MAX_SAFETY_COPIES_KEEP_NEWEST, MAX_VOICE_GATE_HOLD_MS, MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES,
    MIN_BACKUP_SCHEDULE_KEEP_DAILY_DAYS, MIN_BACKUP_SCHEDULE_KEEP_NEWEST,
    MIN_CHAT_HISTORY_DISPLAY_LIMIT, MIN_CHAT_HISTORY_PER_VIEWER_LIMIT,
    MIN_SAFETY_COPIES_DELETE_AFTER_DAYS, MIN_SAFETY_COPIES_KEEP_NEWEST, MIN_VOICE_GATE_HOLD_MS,
    SAFETY_COPIES_MODE_KEEP_NEWEST, SAFETY_COPIES_MODES, SILENT_LEVEL,
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
    "event_log_retention_days",
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
    "action_history_retention_days",
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
    "audio.voice_gate_enabled",
    Flag { default: false },
    Portable,
    Verbatim,
);

pub const AUDIO_VOICE_GATE_THRESHOLD: Setting<Level> = Setting::new(
    "audio.voice_gate_threshold",
    Level {
        default: VOICE_GATE_DEFAULT_THRESHOLD,
        min: SILENT_LEVEL,
        max: FULL_LEVEL,
    },
    Portable,
    Verbatim,
);

pub const AUDIO_VOICE_GATE_HOLD_MS: Setting<Count> = Setting::new(
    "audio.voice_gate_hold_ms",
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
    "chat_history.per_viewer_limit",
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
    "chat_history.display_limit",
    Count {
        default: DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT,
        min: MIN_CHAT_HISTORY_DISPLAY_LIMIT,
        max: MAX_CHAT_HISTORY_DISPLAY_LIMIT,
        unclamped: None,
    },
    Portable,
    Verbatim,
);

pub const TTS_MASTER_VOLUME: Setting<Level> =
    Setting::new("tts.master_volume", FULL_SCALE_VOLUME, Portable, Verbatim);

pub const DIAGNOSTICS_LOG_LEVEL: Setting<Verbosity> = Setting::new(
    "diagnostics.log_level",
    Verbosity {
        default: DEFAULT_DIAGNOSTIC_LOG_LEVEL,
    },
    Portable,
    Verbatim,
);

const LANGUAGE_CODES: &[&str] = &["en", "uk"];
const DEFAULT_LANGUAGE_CODE: &str = "en";

pub const LANGUAGE: Setting<Choice> = Setting::new(
    "app.language",
    Choice {
        default: DEFAULT_LANGUAGE_CODE,
        options: LANGUAGE_CODES,
    },
    Portable,
    Verbatim,
);

pub const REGION: Setting<OptionalText> =
    Setting::new("app.region", OptionalText, Portable, Verbatim);

pub const DATE_FORMAT: Setting<OptionalText> =
    Setting::new("app.date_format", OptionalText, Portable, Verbatim);

pub const TIME_FORMAT: Setting<OptionalText> =
    Setting::new("app.time_format", OptionalText, Portable, Verbatim);

pub const FIRST_DAY_OF_WEEK: Setting<OptionalText> =
    Setting::new("app.first_day_of_week", OptionalText, Portable, Verbatim);

pub const THEME: Setting<OptionalText> = Setting::new("theme", OptionalText, Portable, Verbatim);

pub const ACCENT: Setting<OptionalText> = Setting::new("accent", OptionalText, Portable, Verbatim);

pub const AUDIO_OVERLAY_ID: Setting<OptionalText> =
    Setting::new("audio.overlay_id", OptionalText, Portable, Presence);

pub const FONT_BODY: Setting<OptionalText> =
    Setting::new("font_body", OptionalText, Portable, Presence);

pub const FONT_MONO: Setting<OptionalText> =
    Setting::new("font_mono", OptionalText, Portable, Presence);

pub const AUDIO_OUTPUT_DEVICE_ID: Setting<OptionalText> = Setting::new(
    "audio.output_device_id",
    OptionalText,
    MachineLocal,
    Presence,
);

pub const AUDIO_VOICE_GATE_INPUT_DEVICE_ID: Setting<OptionalText> = Setting::new(
    "audio.voice_gate_input_device_id",
    OptionalText,
    MachineLocal,
    Presence,
);

pub const SERVER_OVERLAY_ROOT: Setting<OptionalText> =
    Setting::new("server.overlay_root", OptionalText, MachineLocal, Presence);

pub const TTS_DISABLED_ENGINES: Setting<JsonDocument<Vec<String>>> = Setting::new(
    "tts.disabled_engines",
    JsonDocument::new(),
    Portable,
    Verbatim,
);

pub const TTS_SYNTHESIS_DEFAULTS: Setting<JsonDocument<SynthesisDefaults>> = Setting::new(
    "tts.synthesis_defaults",
    JsonDocument::new(),
    Portable,
    Verbatim,
);

pub const VOICE_ASSIGNMENT_STRATEGY: Setting<JsonDocument<AssignmentStrategy>> = Setting::new(
    "voice:strategy",
    JsonDocument::new(),
    RepositoryOwned,
    Withheld,
);

pub const CREDENTIALS_KEY_LOSS: Setting<Tally> = Setting::new(
    "credentials.key_loss_stranded",
    Tally { default: 0 },
    MachineLocal,
    Verbatim,
);

pub const TTS_ENGINE_PARAMS: SettingFamily<JsonDocument<EngineParams>> = SettingFamily::new(
    "tts.engine_params.",
    JsonDocument::new(),
    Portable,
    Verbatim,
);

pub const INTEGRATION_ENABLED: SettingFamily<Flag> = SettingFamily::new(
    "integration.enabled.",
    Flag { default: false },
    Portable,
    Verbatim,
);

pub const PICKER_FAVORITES_SUB_ACTIONS: Setting<JsonDocument<Vec<String>>> = Setting::new(
    "picker.favorites.sub_actions",
    JsonDocument::new(),
    Portable,
    Kind,
);

pub const PICKER_FAVORITES_TRIGGERS: Setting<JsonDocument<Vec<String>>> = Setting::new(
    "picker.favorites.triggers",
    JsonDocument::new(),
    Portable,
    Kind,
);

pub const PICKER_FAVORITES_ICONS: Setting<JsonDocument<Vec<String>>> = Setting::new(
    "picker.favorites.icons",
    JsonDocument::new(),
    Portable,
    Kind,
);

pub const BACKUP_SCHEDULE_ENABLED: Setting<Flag> = Setting::new(
    "backup_schedule.enabled",
    Flag {
        default: DEFAULT_BACKUP_SCHEDULE_ENABLED,
    },
    MachineLocal,
    Withheld,
);

pub const BACKUP_SCHEDULE_INTERVAL_MINUTES: Setting<Count> = Setting::new(
    "backup_schedule.interval_minutes",
    Count {
        default: DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES,
        min: MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES,
        max: MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES,
        unclamped: None,
    },
    Portable,
    Withheld,
);

pub const BACKUP_SCHEDULE_KEEP_NEWEST: Setting<Count> = Setting::new(
    "backup_schedule.keep_newest",
    Count {
        default: DEFAULT_BACKUP_SCHEDULE_KEEP_NEWEST,
        min: MIN_BACKUP_SCHEDULE_KEEP_NEWEST,
        max: MAX_BACKUP_SCHEDULE_KEEP_NEWEST,
        unclamped: None,
    },
    Portable,
    Withheld,
);

pub const BACKUP_SCHEDULE_KEEP_DAILY_DAYS: Setting<Count> = Setting::new(
    "backup_schedule.keep_daily_days",
    Count {
        default: DEFAULT_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
        min: MIN_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
        max: MAX_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
        unclamped: None,
    },
    Portable,
    Withheld,
);

const INCLUDED_BY_DEFAULT: Flag = Flag {
    default: DEFAULT_BACKUP_SCHEDULE_INCLUDE_CATEGORY,
};

pub const BACKUP_SCHEDULE_INCLUDE_MEDIA: Setting<Flag> = Setting::new(
    "backup_schedule.include_media",
    INCLUDED_BY_DEFAULT,
    Portable,
    Withheld,
);

pub const BACKUP_SCHEDULE_INCLUDE_VOICES: Setting<Flag> = Setting::new(
    "backup_schedule.include_voices",
    INCLUDED_BY_DEFAULT,
    Portable,
    Withheld,
);

pub const BACKUP_SCHEDULE_INCLUDE_OVERLAYS: Setting<Flag> = Setting::new(
    "backup_schedule.include_overlays",
    INCLUDED_BY_DEFAULT,
    Portable,
    Withheld,
);

pub const BACKUP_SCHEDULE_INCLUDE_ASSETS: Setting<Flag> = Setting::new(
    "backup_schedule.include_assets",
    INCLUDED_BY_DEFAULT,
    Portable,
    Withheld,
);

pub const BACKUP_SCHEDULE_DESTINATION: Setting<OptionalText> = Setting::new(
    "backup_schedule.destination",
    OptionalText,
    MachineLocal,
    Withheld,
);

pub const BACKUP_SCHEDULE_KEY: Setting<OptionalText> =
    Setting::new("backup_schedule.key", OptionalText, MachineLocal, Withheld);

pub const BACKUP_SCHEDULE_LAST_SUCCESS_AT: Setting<OptionalText> = Setting::new(
    "backup_schedule.last_success_at",
    OptionalText,
    MachineLocal,
    Withheld,
);

pub const BACKUP_SCHEDULE_CONTENT_FINGERPRINT: Setting<OptionalText> = Setting::new(
    "backup_schedule.content_fingerprint",
    OptionalText,
    MachineLocal,
    Withheld,
);

pub const SAFETY_COPIES_MODE: Setting<Choice> = Setting::new(
    "safety_copies.mode",
    Choice {
        default: SAFETY_COPIES_MODE_KEEP_NEWEST,
        options: SAFETY_COPIES_MODES,
    },
    MachineLocal,
    Withheld,
);

pub const SAFETY_COPIES_KEEP_NEWEST: Setting<Count> = Setting::new(
    "safety_copies.keep_newest",
    Count {
        default: DEFAULT_SAFETY_COPIES_KEEP_NEWEST,
        min: MIN_SAFETY_COPIES_KEEP_NEWEST,
        max: MAX_SAFETY_COPIES_KEEP_NEWEST,
        unclamped: None,
    },
    MachineLocal,
    Withheld,
);

pub const SAFETY_COPIES_DELETE_AFTER_DAYS: Setting<Count> = Setting::new(
    "safety_copies.delete_after_days",
    Count {
        default: DEFAULT_SAFETY_COPIES_DELETE_AFTER_DAYS,
        min: MIN_SAFETY_COPIES_DELETE_AFTER_DAYS,
        max: MAX_SAFETY_COPIES_DELETE_AFTER_DAYS,
        unclamped: None,
    },
    MachineLocal,
    Withheld,
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
    PICKER_FAVORITES_SUB_ACTIONS.spec(),
    PICKER_FAVORITES_TRIGGERS.spec(),
    PICKER_FAVORITES_ICONS.spec(),
    TTS_DISABLED_ENGINES.spec(),
    TTS_SYNTHESIS_DEFAULTS.spec(),
    TTS_MASTER_VOLUME.spec(),
    TTS_ENGINE_PARAMS.spec(),
    DIAGNOSTICS_LOG_LEVEL.spec(),
    CREDENTIALS_KEY_LOSS.spec(),
    SERVER_OVERLAY_ROOT.spec(),
    INTEGRATION_ENABLED.spec(),
    VOICE_ASSIGNMENT_STRATEGY.spec(),
    BACKUP_SCHEDULE_ENABLED.spec(),
    BACKUP_SCHEDULE_INTERVAL_MINUTES.spec(),
    BACKUP_SCHEDULE_KEEP_NEWEST.spec(),
    BACKUP_SCHEDULE_KEEP_DAILY_DAYS.spec(),
    BACKUP_SCHEDULE_INCLUDE_MEDIA.spec(),
    BACKUP_SCHEDULE_INCLUDE_VOICES.spec(),
    BACKUP_SCHEDULE_INCLUDE_OVERLAYS.spec(),
    BACKUP_SCHEDULE_INCLUDE_ASSETS.spec(),
    BACKUP_SCHEDULE_DESTINATION.spec(),
    BACKUP_SCHEDULE_KEY.spec(),
    BACKUP_SCHEDULE_LAST_SUCCESS_AT.spec(),
    BACKUP_SCHEDULE_CONTENT_FINGERPRINT.spec(),
    SAFETY_COPIES_MODE.spec(),
    SAFETY_COPIES_KEEP_NEWEST.spec(),
    SAFETY_COPIES_DELETE_AFTER_DAYS.spec(),
];
