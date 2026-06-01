pub(crate) mod synth;

#[cfg(not(target_os = "macos"))]
mod stub;

#[cfg(target_os = "macos")]
mod engine;
#[cfg(target_os = "macos")]
pub(crate) mod error;
#[cfg(target_os = "macos")]
pub(crate) mod voices;

#[cfg(target_os = "macos")]
pub use engine::NsSpeechEngineFactory;

#[cfg(not(target_os = "macos"))]
pub use stub::NsSpeechEngineFactory;
