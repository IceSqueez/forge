mod backend;
#[cfg(target_os = "linux")]
mod backend_linux;
#[cfg(target_os = "macos")]
mod backend_macos;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod backend_unsupported;
#[cfg(target_os = "windows")]
mod backend_windows;
mod error;
mod handle;
mod status;
mod supervisor;

pub use error::AwakeError;
pub use handle::{EnabledPreference, StatusWatch, StayAwake, StayAwakeConfig};
pub use status::{Aspect, AspectState, AwakeStatus};
