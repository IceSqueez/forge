pub mod backend;
pub mod client;
pub mod combo;
pub mod config;
pub(crate) mod content;
pub(crate) mod control;
pub mod error;
pub(crate) mod health;
mod payload_fields;
pub mod quick_actions;
pub(crate) mod status;
pub(crate) mod supervisor;
pub mod triggers;

#[cfg(target_os = "linux")]
pub(crate) mod backend_evdev;
#[cfg(target_os = "linux")]
pub(crate) mod backend_portal;

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) mod backend_global;

pub use backend::HotkeyId;
pub use client::{EnableFailure, HotkeyClient};
pub use combo::HotkeyCombo;
pub use config::HotkeyConfig;
pub use error::HotkeyError;
pub use triggers::register_hotkey_triggers;
