#![doc = "DataProvider trait + per-domain repo traits. Backend-agnostic storage contract."]

pub mod action;
pub mod ban_ledger;
mod catalog;
pub mod chat_history;
pub mod credentials;
pub mod donation;
mod error;
pub mod event_log;
pub mod globals;
pub mod history;
mod integration_state;
pub mod latest_value;
pub mod media;
pub mod overlay;
mod provider;
pub mod queue;
pub mod scheduled_run;
mod schema_file;
pub mod script;
pub mod settings;
pub mod soundboard;
#[cfg(feature = "settings-test-support")]
pub mod test_support;
mod transit;
pub mod trigger_instance;
pub mod tts_filters;
pub mod user_globals;
pub mod viewer;
pub mod voice_aliases;

pub use action::{ActionExecution, ActionRepo, ActionTelemetry, ExecutionStatus};
pub use ban_ledger::{
    BAN_LEDGER_LIST_CAP, BAN_TERM_MATCH_TOLERANCE, BanLedgerEntry, BanLedgerKey, BanLedgerRepo,
    BanOrigin, merge_ban_entry,
};
pub use catalog::{
    CatalogChanges, CatalogRevision, RevisingActionRepo, RevisingQueueRepo, RevisingScriptRepo,
    RevisingTriggerInstanceRepo,
};
pub use chat_history::{
    AUTHORLESS_CHAT_HISTORY_RETAINED, ChatAuthorKey, ChatAuthorPage, ChatAuthorTally,
    ChatHistoryCursor, ChatHistoryRepo, ChatModerationTarget,
};
pub use credentials::{
    CredentialId, CredentialsRepo, SERVER_BEARER_CREDENTIAL_ID, SERVER_OVERLAY_TOKEN_CREDENTIAL_ID,
    is_regenerable_credential,
};
pub use donation::{DonationRepo, StoredDonation};
pub use error::StorageError;
pub use event_log::{
    DEFAULT_EVENT_LOG_RETENTION_DAYS, EventLogRepo, MAX_EVENT_LOG_RETENTION_DAYS,
    MIN_EVENT_LOG_RETENTION_DAYS, clamp_event_log_retention_days, event_log_retention_days,
    set_event_log_retention_days,
};
pub use globals::{GlobalEntry, GlobalsRepo};
pub use history::{
    ActionStats, HistoryRepo, MAX_ACTION_HISTORY_RETENTION_DAYS, MIN_ACTION_HISTORY_RETENTION_DAYS,
    action_history_retention_days, clamp_action_history_retention_days,
    set_action_history_retention_days,
};
pub use integration_state::{
    has_credentials_for, integration_enabled_key, resolve_integration_enabled,
    set_integration_enabled, stored_integration_enabled,
};
pub use latest_value::{LatestRecord, LatestValueRepo};
pub use media::{
    AcceptedMedia, MAX_AUDIO_BLOB_BYTES, MAX_IMAGE_BLOB_BYTES, MEDIA_BLOB_HARD_CEILING_BYTES,
    MEDIA_CONTENT_DIGEST_BYTES, MEDIA_CONTENT_HASH, MediaBlob, MediaBlobId, MediaFormat, MediaKind,
    MediaReferrer, MediaReferrerKind, MediaRepo, accept_media, sanitize_label, sniff,
};
pub use overlay::{
    OVERLAY_TOKEN_PREFIX, OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId,
    OverlayRepo, is_token_shaped,
};
pub use provider::{
    DATABASE_FILE_NAME, DataProvider, EXPECTED_SCHEMA_VERSION, LAST_PRE_BASELINE_RELEASE,
    database_unit_file_names,
};
pub use queue::QueueRepo;
pub use scheduled_run::{
    ACTION_REMOVED_REASON, CANCELLED_REASON, MissedRunPolicy, RevisingScheduledRunRepo,
    SUPERSEDED_REASON, ScheduledRun, ScheduledRunId, ScheduledRunOutcome, ScheduledRunPlacement,
    ScheduledRunRepo, ScheduledRunSpec, ScheduledRunState,
};
pub use schema_file::{DatabaseFileSchema, StoredDatabase, StoredSchema};
pub use script::{
    SCRIPT_NAME_KEY, SCRIPT_RUN_NAMED_KIND, ScriptRecord, ScriptReferences, ScriptReferrer,
    ScriptRepo, ScriptTelemetry,
};
pub use settings::catalog::{SettingSkip, SettingsCatalog};
pub use settings::disclosure::{SettingDisclosure, disclose_rows};
pub use settings::favorites::rename_favorite_ids;
pub use settings::portability::SettingPortability;
pub use settings::row::{
    RowClasses, RowGate, SettingRow, load_setting_rows, remove_row, restore_row,
};
pub use settings::spec::{SettingEncoding, SettingKey, SettingSpec};
pub use settings::typed::{
    Choice, Count, Flag, JsonDocument, Level, OptionalText, PlainText, Setting, SettingCodec,
    SettingFamily, Tally, Verbosity, family_members, read_family_setting, read_setting,
    read_setting_or_default, read_stored_family_setting, read_stored_setting, reset_family_setting,
    reset_setting, write_family_setting, write_setting,
};
pub use settings::{
    CredentialsKeyLoss, DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT, DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT,
    DEFAULT_DIAGNOSTIC_LOG_LEVEL, EngineParams, Language, MAX_CHAT_HISTORY_DISPLAY_LIMIT,
    MAX_CHAT_HISTORY_PER_VIEWER_LIMIT, MAX_VOICE_GATE_HOLD_MS, MIN_CHAT_HISTORY_DISPLAY_LIMIT,
    MIN_CHAT_HISTORY_PER_VIEWER_LIMIT, MIN_VOICE_GATE_HOLD_MS,
    SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT, SettingsRepo, StoredDisplayFormats,
    UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT, UnknownLanguage, VOICE_GATE_DEFAULT_HOLD_MS,
    VOICE_GATE_DEFAULT_THRESHOLD, VoiceGateSettings, chat_history_display_limit,
    chat_history_per_viewer_limit, clamp_chat_history_per_viewer_limit, diagnostic_log_level,
    disabled_tts_engines, disclosure, engine_params, log_level_as_str, master_volume,
    record_credentials_key_loss, set_chat_history_display_limit, set_chat_history_per_viewer_limit,
    set_diagnostic_log_level, set_disabled_tts_engines, set_engine_params, set_master_volume,
    set_voice_gate_enabled, set_voice_gate_hold_ms, set_voice_gate_input_device_id,
    set_voice_gate_threshold, stored_display_formats, synthesis_defaults,
    take_credentials_key_loss, voice_gate_settings,
};
pub use soundboard::{CLIP_SOURCE_SLOT, SoundboardClipsRepo, StoredClip, clip_source_referrer};
pub use transit::{CURRENT_FORMAT_VERSION, GlobalTransit, GlobalsExport};
pub use trigger_instance::TriggerInstanceRepo;
pub use tts_filters::{
    BlocklistMode, FilterRule, FilterRuleKind, TtsFiltersRepo, TtsPipelineSettings, UrlMode,
};
pub use user_globals::{UserGlobalEntry, UserGlobalsRepo};
pub use viewer::{Viewer, ViewerMessage, ViewerRepo};
pub use voice_aliases::VoiceAliasRepo;

#[cfg(feature = "test-mocks")]
pub use ban_ledger::MockBanLedgerRepo;
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
