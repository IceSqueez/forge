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
#[cfg(test)]
mod fake_backend;
mod handle;
mod messages;
mod status;
mod supervisor;

pub use error::{AwakeError, AwakeService};
pub use handle::{StatusWatch, StayAwake, StayAwakeConfig};
pub use messages::AWAKE_MESSAGES;
pub use status::{Aspect, AspectState, AwakeStatus};
