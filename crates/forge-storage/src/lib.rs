#![doc = "DataProvider trait + per-domain repo traits. Backend-agnostic storage contract."]

mod action;
mod ban_ledger;
mod catalog;
mod chat_history;
pub mod credentials;
mod donation;
mod error;
mod event_log;
mod globals;
mod history;
mod integration_state;
mod latest_value;
mod media;
mod overlay;
mod provider;
mod queue;
mod scheduled_run;
mod schema_file;
mod script;
pub mod settings;
mod soundboard;
#[cfg(feature = "settings-test-support")]
pub mod test_support;
mod transit;
mod trigger_instance;
mod tts_filters;
mod unit_of_work;
mod user_globals;
mod viewer;
mod voice_aliases;

pub use action::{ActionExecution, ActionRepo, ActionTelemetry, ExecutionStatus};
pub use ban_ledger::{
    BAN_LEDGER_LIST_CAP, BanLedgerEntry, BanLedgerKey, BanLedgerRepo, BanOrigin, merge_ban_entry,
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
    MIN_EVENT_LOG_RETENTION_DAYS, event_log_retention_days, set_event_log_retention_days,
};
pub use globals::{GlobalEntry, GlobalsRepo};
pub use history::{
    ActionStats, HistoryRepo, MAX_ACTION_HISTORY_RETENTION_DAYS, MIN_ACTION_HISTORY_RETENTION_DAYS,
    action_history_retention_days, clamp_action_history_retention_days,
    set_action_history_retention_days,
};
pub use integration_state::{
    has_credentials_for, integration_enabled_key, resolve_integration_enabled,
    set_integration_enabled,
};
pub use latest_value::{LatestRecord, LatestValueRepo};
pub use media::{
    MEDIA_CONTENT_DIGEST_BYTES, MediaBlob, MediaBlobId, MediaFormat, MediaKind, MediaReferrer,
    MediaReferrerKind, MediaRepo, accept_media, sanitize_label, sniff,
};
pub use overlay::{
    NewOverlay, OVERLAY_TOKEN_PREFIX, OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo,
    is_token_shaped, unique_overlay_id,
};
pub use provider::{
    DATABASE_FILE_NAME, DataProvider, EXPECTED_SCHEMA_VERSION, LAST_PRE_BASELINE_RELEASE,
    database_unit_file_names,
};
pub use queue::{OVERLAY_ALERT_QUEUE_NAME, QueueRepo, QueueRole};
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
pub use settings::row::{RowClasses, RowGate, SettingRow, load_setting_rows};
pub use settings::spec::{SettingEncoding, SettingKey, SettingSpec};
pub use settings::typed::{
    Choice, Count, Flag, JsonDocument, Level, OptionalText, PlainText, Setting, SettingCodec,
    SettingFamily, Tally, read_family_setting, read_setting, read_setting_or_default,
    read_stored_setting, reset_family_setting, reset_setting, setting_row, write_family_setting,
    write_setting,
};
pub use settings::{
    CredentialsKeyLoss, DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT, DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT,
    DEFAULT_DIAGNOSTIC_LOG_LEVEL, EngineParams, Language, MAX_CHAT_HISTORY_PER_VIEWER_LIMIT,
    MIN_CHAT_HISTORY_PER_VIEWER_LIMIT, SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT, SettingsRepo,
    UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT, VOICE_GATE_DEFAULT_HOLD_MS,
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
pub use transit::{GlobalTransit, GlobalsExport};
pub use trigger_instance::{TriggerChanges, TriggerInstanceRepo, TriggerWiring};
pub use tts_filters::{
    BlocklistMode, FilterRule, FilterRuleKind, SkipRegex, TtsFiltersRepo, TtsPipelineSettings,
    UrlMode,
};
pub use unit_of_work::StoreChange;
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

#[cfg(feature = "test-mocks")]
pub use action::MockActionRepo;
#[cfg(feature = "test-mocks")]
pub use chat_history::MockChatHistoryRepo;
#[cfg(feature = "test-mocks")]
pub use event_log::MockEventLogRepo;
#[cfg(feature = "test-mocks")]
pub use globals::MockGlobalsRepo;
pub use globals::{append_bounded, remove_matching, type_mismatch};
#[cfg(feature = "test-mocks")]
pub use history::MockHistoryRepo;
pub use media::AcceptedMedia;
#[cfg(feature = "test-mocks")]
pub use queue::MockQueueRepo;
#[cfg(feature = "test-mocks")]
pub use script::MockScriptRepo;
#[cfg(feature = "test-mocks")]
pub use soundboard::MockSoundboardClipsRepo;
#[cfg(feature = "test-mocks")]
pub use tts_filters::MockTtsFiltersRepo;
#[cfg(feature = "test-mocks")]
pub use user_globals::MockUserGlobalsRepo;
pub use user_globals::incremented;
#[cfg(feature = "test-mocks")]
pub use viewer::MockViewerRepo;
#[cfg(feature = "test-mocks")]
pub use voice_aliases::MockVoiceAliasRepo;
