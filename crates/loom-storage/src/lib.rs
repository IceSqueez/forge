#![doc = "DataProvider trait + per-domain repo traits. Backend-agnostic storage contract."]

pub mod action;
pub mod command;
pub mod error;
pub mod globals;
pub mod settings;
pub mod trigger;
pub mod user_globals;

pub use action::{ActionRecord, ActionRepo};
pub use command::{CommandRecord, CommandRepo};
pub use error::StorageError;
pub use globals::{GlobalEntry, GlobalsRepo};
pub use settings::{SettingsRepo, reserved_keys};
pub use trigger::{TriggerRecord, TriggerRepo};
pub use user_globals::{UserGlobalEntry, UserGlobalsRepo};
