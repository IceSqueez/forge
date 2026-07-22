#![doc = "DataProvider trait + per-domain repo traits. Backend-agnostic storage contract."]

pub mod action;
pub mod chat_history;
pub mod credentials;
pub mod error;
pub mod event_log;
pub mod globals;
pub mod history;
pub mod provider;
pub mod queue;
pub mod script;
pub mod settings;
pub mod soundboard;
pub mod transit;
pub mod trigger_instance;
pub mod tts_filters;
pub mod user_globals;
pub mod viewer;
pub mod voice_aliases;

pub use action::{ActionRepo, ActionTelemetry, ExecutionStatus};
pub use chat_history::ChatHistoryRepo;
pub use credentials::{CredentialId, CredentialsRepo};
pub use error::StorageError;
pub use event_log::{EventLogRepo, event_log_retention_days, set_event_log_retention_days};
pub use globals::{GlobalEntry, GlobalsRepo};
pub use history::{ActionStats, HistoryRepo};
pub use provider::{DataProvider, EXPECTED_SCHEMA_VERSION};
pub use queue::QueueRepo;
pub use script::{ScriptRecord, ScriptRepo, ScriptTelemetry};
pub use settings::{
    EngineParams, Language, SettingsRepo, UnknownLanguage, chat_history_display_limit,
    chat_history_store_limit, disabled_tts_engines, engine_params, get_bool_setting,
    get_json_setting, master_volume, reserved_keys, set_bool_setting,
    set_chat_history_display_limit, set_chat_history_store_limit, set_disabled_tts_engines,
    set_engine_params, set_json_setting, set_master_volume, set_soundboard_also_headphones,
    set_soundboard_enabled, set_soundboard_master_volume, set_soundboard_output_device,
    soundboard_also_headphones, soundboard_enabled, soundboard_master_volume,
    soundboard_output_device, synthesis_defaults,
};
pub use soundboard::{SoundboardClipsRepo, StoredClip};
pub use transit::{CURRENT_FORMAT_VERSION, GlobalTransit, GlobalsExport};
pub use trigger_instance::TriggerInstanceRepo;
pub use tts_filters::{
    BlocklistMode, FilterRule, FilterRuleKind, TtsFiltersRepo, TtsPipelineSettings, UrlMode,
};
pub use user_globals::{UserGlobalEntry, UserGlobalsRepo};
pub use viewer::{Viewer, ViewerPlatform, ViewerRepo};
pub use voice_aliases::{AliasId, AssignmentStrategy, IgnoreProfile, VoiceAlias, VoiceAliasRepo};

#[cfg(feature = "test-mocks")]
pub use trigger_instance::MockTriggerInstanceRepo;
