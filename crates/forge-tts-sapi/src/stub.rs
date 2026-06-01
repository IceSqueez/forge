use forge_tts_core::{EngineId, TtsEngine, TtsEngineFactory, TtsError};

pub struct SapiEngineFactory;

impl TtsEngineFactory for SapiEngineFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Err(TtsError::EngineUnavailable {
            id: EngineId("sapi".into()),
            detail: "SAPI 5 is Windows-only".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_returns_engine_unavailable() {
        assert!(matches!(
            SapiEngineFactory.create(),
            Err(TtsError::EngineUnavailable { .. })
        ));
    }
}
