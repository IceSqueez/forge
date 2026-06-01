use std::sync::{Arc, mpsc};

use async_trait::async_trait;
use forge_tts_core::{
    EngineCapabilities, EngineId, PcmBuffer, SynthesisRequest, TtsEngine, TtsEngineFactory,
    TtsError, TtsVoice,
};

use crate::com::{StaRequest, spawn_sta_worker};
use crate::error::SapiError;

static CAPABILITIES: EngineCapabilities = EngineCapabilities {
    ssml: true,
    neural_voices: false,
    streaming: false,
    custom_lexicons: false,
};

pub struct SapiEngine {
    id: EngineId,
    sta_tx: mpsc::Sender<StaRequest>,
    voice_catalog: Arc<Vec<TtsVoice>>,
}

#[async_trait]
impl TtsEngine for SapiEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }

    fn capabilities(&self) -> &EngineCapabilities {
        &CAPABILITIES
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        Ok((*self.voice_catalog).clone())
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        if !self.voice_catalog.iter().any(|v| v.id == request.voice_id) {
            return Err(TtsError::InvalidVoice {
                id: request.voice_id,
            });
        }

        let (oneshot_tx, oneshot_rx) = tokio::sync::oneshot::channel();
        self.sta_tx
            .send(StaRequest::Synthesize {
                voice_id: request.voice_id.clone(),
                req: request,
                tx: oneshot_tx,
            })
            .map_err(|_| TtsError::EngineUnavailable {
                id: self.id.clone(),
                detail: "STA worker terminated".into(),
            })?;

        oneshot_rx
            .await
            .map_err(|_| TtsError::EngineUnavailable {
                id: self.id.clone(),
                detail: "STA worker dropped request".into(),
            })?
            .map_err(TtsError::from)
    }
}

pub struct SapiEngineFactory;

impl TtsEngineFactory for SapiEngineFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        let engine_id = EngineId("sapi".into());
        let (sta_tx, catalog) = spawn_sta_worker(engine_id.clone()).map_err(TtsError::from)?;

        if catalog.is_empty() {
            return Err(TtsError::EngineUnavailable {
                id: engine_id,
                detail: "no SAPI 5 voices found".into(),
            });
        }

        tracing::info!(voices = catalog.len(), "SAPI 5 engine ready");
        Ok(Box::new(SapiEngine {
            id: engine_id,
            sta_tx,
            voice_catalog: Arc::new(catalog),
        }))
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use forge_tts_core::VoiceId;

    #[tokio::test]
    async fn list_voices_non_empty_on_windows() {
        let engine = SapiEngineFactory.create().expect("SAPI must be available");
        let voices = engine.list_voices().await.expect("list voices");
        assert!(!voices.is_empty(), "expected at least one SAPI voice");
    }

    #[tokio::test]
    async fn synthesize_produces_pcm_for_first_voice() {
        let engine = SapiEngineFactory.create().expect("SAPI must be available");
        let voices = engine.list_voices().await.expect("list voices");
        let first = voices.first().expect("at least one voice");
        let req = SynthesisRequest {
            text: "test".into(),
            voice_id: first.id.clone(),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: false,
        };
        let buf = engine.synthesize(req).await.expect("synthesize");
        assert!(!buf.samples.is_empty());
        assert!(matches!(buf.sample_rate, 16_000 | 22_050));
        assert_eq!(buf.channels, 1);
    }

    #[tokio::test]
    async fn synthesize_returns_invalid_voice_for_unknown_id() {
        let engine = SapiEngineFactory.create().expect("SAPI must be available");
        let req = SynthesisRequest {
            text: "test".into(),
            voice_id: VoiceId("nonexistent/sapi/voice".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: false,
        };
        let result = engine.synthesize(req).await;
        assert!(matches!(result, Err(TtsError::InvalidVoice { .. })));
    }
}
