use forge_tts_core::{EngineId, TtsEngine, TtsEngineFactory, TtsError};

use crate::process;

fn espeak_engine_id() -> EngineId {
    EngineId("espeak-ng".into())
}

pub struct EspeakEngineFactory;

impl TtsEngineFactory for EspeakEngineFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        process::check_espeak_version().map_err(TtsError::from)?;
        Err(TtsError::EngineUnavailable {
            id: espeak_engine_id(),
            detail: "synthesis engine not yet wired".into(),
        })
    }
}
