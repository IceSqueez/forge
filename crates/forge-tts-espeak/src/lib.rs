mod engine;
mod error;
mod process;
mod voices;

pub use engine::{EspeakEngine, EspeakEngineFactory};
pub use process::{pitch_from_semitones, rate_wpm_from_multiplier};
pub use voices::parse_voices_output;
