use std::sync::{Arc, mpsc};

use async_trait::async_trait;
use forge_tts_core::{
    EngineCapabilities, EngineId, PcmBuffer, SynthesisRequest, TtsEngine, TtsEngineFactory,
    TtsError, TtsVoice,
};

use crate::com::{StaRequest, spawn_sta_worker};

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
