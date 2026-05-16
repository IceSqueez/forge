#![doc = "DataProvider trait + per-domain repo traits. Backend-agnostic storage contract."]

pub mod error;
pub mod globals;
pub mod user_globals;

pub use error::StorageError;
pub use globals::{GlobalEntry, GlobalsRepo};
pub use user_globals::{UserGlobalEntry, UserGlobalsRepo};
