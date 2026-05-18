#![doc = "DataProvider trait + per-domain repo traits. Backend-agnostic storage contract."]

pub mod action;
pub mod command;
pub mod credentials;
pub mod error;
pub mod globals;
pub mod history;
pub mod provider;
pub mod queue;
pub mod script;
pub mod settings;
pub mod transit;
pub mod trigger;
pub mod user_globals;

pub use action::ActionRepo;
pub use command::CommandRepo;
pub use credentials::{CredentialId, CredentialsRepo};
pub use error::StorageError;
pub use globals::{GlobalEntry, GlobalsRepo};
pub use history::HistoryRepo;
pub use provider::DataProvider;
pub use queue::QueueRepo;
pub use script::{ScriptRecord, ScriptRepo};
pub use settings::{SettingsRepo, reserved_keys};
pub use transit::{CURRENT_FORMAT_VERSION, GlobalTransit, GlobalsExport};
pub use trigger::TriggerRepo;
pub use user_globals::{UserGlobalEntry, UserGlobalsRepo};
