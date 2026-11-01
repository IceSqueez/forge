use forge_speech_types::EngineId;
use forge_tts_core::{
    ENGINE_ID_AVSPEECH_STORED_AS_NSSPEECH, TtsEngine, TtsEngineFactory, TtsError,
};

pub struct AvSpeechEngineFactory;

impl TtsEngineFactory for AvSpeechEngineFactory {
    fn engine_id(&self) -> forge_speech_types::EngineId {
        forge_speech_types::EngineId(forge_tts_core::ENGINE_ID_AVSPEECH_STORED_AS_NSSPEECH.into())
    }

    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::System
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Err(TtsError::EngineUnavailable {
            id: EngineId(ENGINE_ID_AVSPEECH_STORED_AS_NSSPEECH.into()),
            detail: "AVSpeech is macOS-only".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_returns_engine_unavailable() {
        assert!(matches!(
            AvSpeechEngineFactory.create(),
            Err(TtsError::EngineUnavailable { .. })
        ));
    }
}
