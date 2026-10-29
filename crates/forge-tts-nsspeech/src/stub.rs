use forge_speech_types::EngineId;
use forge_tts_core::{ENGINE_ID_NSSPEECH, TtsEngine, TtsEngineFactory, TtsError};

pub struct NsSpeechEngineFactory;

impl TtsEngineFactory for NsSpeechEngineFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::System
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Err(TtsError::EngineUnavailable {
            id: EngineId(ENGINE_ID_NSSPEECH.into()),
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
