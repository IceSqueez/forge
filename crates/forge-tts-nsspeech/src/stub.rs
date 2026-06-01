use forge_tts_core::{EngineId, TtsEngine, TtsEngineFactory, TtsError};

pub struct NsSpeechEngineFactory;

impl TtsEngineFactory for NsSpeechEngineFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Err(TtsError::EngineUnavailable {
            id: EngineId("nsspeech".into()),
            detail: "AVFoundation is macOS-only".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_returns_engine_unavailable() {
        assert!(matches!(
            NsSpeechEngineFactory.create(),
            Err(TtsError::EngineUnavailable { .. })
        ));
    }
}
