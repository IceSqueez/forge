#![doc = "DataProvider trait + per-domain repo traits. Backend-agnostic storage contract."]

pub mod action;
pub mod catalog;
pub mod chat_history;
pub mod credentials;
pub mod donation;
pub mod error;
pub mod event_log;
pub mod globals;
pub mod history;
pub mod integration_state;
pub mod latest_value;
pub mod media;
pub mod overlay;
pub mod provider;
pub mod queue;
pub mod scheduled_run;
pub mod script;
pub mod settings;
pub mod soundboard;
pub mod transit;
pub mod trigger_instance;
pub mod tts_filters;
pub mod user_globals;
pub mod viewer;
pub mod voice_aliases;

pub use action::{ActionExecution, ActionRepo, ActionTelemetry, ExecutionStatus};
pub use catalog::{
    CatalogChanges, CatalogRevision, RevisingActionRepo, RevisingQueueRepo,
    RevisingTriggerInstanceRepo,
};
pub use chat_history::{AUTHORLESS_CHAT_HISTORY_RETAINED, ChatAuthorKey, ChatHistoryRepo};
pub use credentials::{CredentialId, CredentialsRepo, SERVER_BEARER_CREDENTIAL_ID};
pub use donation::{DonationRepo, StoredDonation};
pub use error::StorageError;
pub use event_log::{
    DEFAULT_EVENT_LOG_RETENTION_DAYS, EventLogRepo, MAX_EVENT_LOG_RETENTION_DAYS,
    MIN_EVENT_LOG_RETENTION_DAYS, clamp_event_log_retention_days, event_log_retention_days,
    set_event_log_retention_days,
};
pub use globals::{GlobalEntry, GlobalsRepo};
pub use history::{ActionStats, HistoryRepo};
pub use integration_state::{
    has_credentials_for, has_setting, integration_enabled_key, resolve_integration_enabled,
    set_integration_enabled, stored_integration_enabled,
};
pub use latest_value::{LatestRecord, LatestValueRepo};
pub use media::{
    AcceptedMedia, MAX_AUDIO_BLOB_BYTES, MAX_IMAGE_BLOB_BYTES, MEDIA_BLOB_HARD_CEILING_BYTES,
    MEDIA_CONTENT_DIGEST_BYTES, MEDIA_CONTENT_HASH, MediaBlob, MediaBlobId, MediaFormat, MediaKind,
    MediaReferrer, MediaReferrerKind, MediaRepo, accept_media, sanitize_label, sniff,
};
pub use overlay::{OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId, OverlayRepo};
pub use provider::{DataProvider, EXPECTED_SCHEMA_VERSION, LAST_PRE_BASELINE_RELEASE};
pub use queue::QueueRepo;
pub use scheduled_run::{
    ACTION_REMOVED_REASON, CANCELLED_REASON, MissedRunPolicy, RevisingScheduledRunRepo,
    SUPERSEDED_REASON, ScheduledRun, ScheduledRunId, ScheduledRunOutcome, ScheduledRunPlacement,
    ScheduledRunRepo, ScheduledRunSpec, ScheduledRunState,
};
pub use script::{ScriptRecord, ScriptRepo, ScriptTelemetry};
pub use settings::{
    CredentialsKeyLoss, DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT, DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT,
    DEFAULT_DIAGNOSTIC_LOG_LEVEL, EngineParams, Language, MAX_CHAT_HISTORY_PER_VIEWER_LIMIT,
    MIN_CHAT_HISTORY_PER_VIEWER_LIMIT, SettingsRepo, UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT,
    UnknownLanguage, VOICE_GATE_DEFAULT_HOLD_MS, VOICE_GATE_DEFAULT_THRESHOLD, VoiceGateSettings,
    chat_history_display_limit, chat_history_per_viewer_limit, clamp_chat_history_per_viewer_limit,
    diagnostic_log_level, disabled_tts_engines, disclosure, engine_params, get_bool_setting,
    get_json_setting, log_level_as_str, master_volume, record_credentials_key_loss, reserved_keys,
    set_bool_setting, set_chat_history_display_limit, set_chat_history_per_viewer_limit,
    set_diagnostic_log_level, set_disabled_tts_engines, set_engine_params, set_json_setting,
    set_master_volume, set_soundboard_also_headphones, set_soundboard_enabled,
    set_soundboard_master_volume, set_soundboard_output_device, set_voice_gate_enabled,
    set_voice_gate_hold_ms, set_voice_gate_input_device_id, set_voice_gate_threshold,
    soundboard_also_headphones, soundboard_enabled, soundboard_master_volume,
    soundboard_output_device, synthesis_defaults, take_credentials_key_loss, voice_gate_settings,
};
pub use soundboard::{CLIP_SOURCE_SLOT, SoundboardClipsRepo, StoredClip, clip_source_referrer};
pub use transit::{CURRENT_FORMAT_VERSION, GlobalTransit, GlobalsExport};
pub use trigger_instance::TriggerInstanceRepo;
pub use tts_filters::{
    BlocklistMode, FilterRule, FilterRuleKind, TtsFiltersRepo, TtsPipelineSettings, UrlMode,
};
pub use user_globals::{UserGlobalEntry, UserGlobalsRepo};
pub use viewer::{Viewer, ViewerMessage, ViewerPlatform, ViewerRepo};
pub use voice_aliases::{AliasId, AssignmentStrategy, IgnoreProfile, VoiceAlias, VoiceAliasRepo};

#[cfg(feature = "test-mocks")]
pub use donation::MockDonationRepo;
#[cfg(feature = "test-mocks")]
pub use latest_value::MockLatestValueRepo;
#[cfg(feature = "test-mocks")]
pub use media::MockMediaRepo;
#[cfg(feature = "test-mocks")]
pub use overlay::MockOverlayRepo;
#[cfg(feature = "test-mocks")]
pub use scheduled_run::MockScheduledRunRepo;
#[cfg(feature = "test-mocks")]
pub use trigger_instance::MockTriggerInstanceRepo;
