pub mod backend;
pub mod client;
pub mod combo;
pub mod config;
pub mod error;
pub(crate) mod health;
pub(crate) mod supervisor;

#[cfg(target_os = "linux")]
pub(crate) mod backend_evdev;
#[cfg(target_os = "linux")]
pub(crate) mod backend_portal;

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) mod backend_global;

pub use backend::HotkeyId;
pub use client::HotkeyClient;
pub use combo::HotkeyCombo;
pub use config::HotkeyConfig;
pub use error::HotkeyError;
