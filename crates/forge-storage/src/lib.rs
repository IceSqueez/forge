#![doc = "DataProvider trait + per-domain repo traits. Backend-agnostic storage contract."]

pub mod action;
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
pub mod trigger;
pub mod trigger_instance;
pub mod user_globals;
pub mod viewer;
pub mod voice_aliases;

pub use action::{ActionRepo, ActionTelemetry};
pub use credentials::{CredentialId, CredentialsRepo};
pub use error::StorageError;
pub use event_log::{EventLogRepo, event_log_retention_days, set_event_log_retention_days};
pub use globals::{GlobalEntry, GlobalsRepo};
pub use history::{ActionStats, HistoryRepo};
pub use provider::DataProvider;
pub use queue::QueueRepo;
pub use script::{ScriptRecord, ScriptRepo};
pub use settings::{SettingsRepo, reserved_keys};
pub use soundboard::{SoundboardClipsRepo, StoredClip};
pub use transit::{CURRENT_FORMAT_VERSION, GlobalTransit, GlobalsExport};
pub use trigger::TriggerRepo;
pub use trigger_instance::TriggerInstanceRepo;
pub use user_globals::{UserGlobalEntry, UserGlobalsRepo};
pub use viewer::{Viewer, ViewerPlatform, ViewerRepo};
pub use voice_aliases::{AliasId, AssignmentStrategy, IgnoreProfile, VoiceAlias, VoiceAliasRepo};

#[cfg(feature = "test-mocks")]
pub use trigger_instance::MockTriggerInstanceRepo;
