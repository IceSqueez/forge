pub(crate) mod synth;

#[cfg(not(target_os = "macos"))]
mod stub;

#[cfg(target_os = "macos")]
pub(crate) mod error;

#[cfg(not(target_os = "macos"))]
pub use stub::NsSpeechEngineFactory;
