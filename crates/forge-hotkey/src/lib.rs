pub mod backend;
pub mod combo;
pub mod config;
pub mod error;

#[cfg(target_os = "linux")]
pub(crate) mod backend_portal;

pub use backend::HotkeyId;
pub use combo::HotkeyCombo;
pub use config::HotkeyConfig;
pub use error::HotkeyError;
