pub(crate) mod synth;
pub(crate) mod voices;

#[cfg(not(target_os = "windows"))]
mod stub;

#[cfg(target_os = "windows")]
mod com;
#[cfg(target_os = "windows")]
mod engine;
#[cfg(target_os = "windows")]
pub(crate) mod error;

#[cfg(target_os = "windows")]
pub use engine::SapiEngineFactory;

#[cfg(not(target_os = "windows"))]
pub use stub::SapiEngineFactory;
