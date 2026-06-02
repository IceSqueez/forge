use std::sync::{Arc, mpsc};

use async_trait::async_trait;
use forge_tts_core::{
    EngineCapabilities, EngineId, PcmBuffer, SynthesisRequest, TtsEngine, TtsEngineFactory,
    TtsError, TtsVoice,
};

use crate::synth::{NsSpeechRequest, spawn_worker};
use crate::voices;

static CAPABILITIES: EngineCapabilities = EngineCapabilities {
    ssml: false,
    neural_voices: true,
    streaming: false,
    custom_lexicons: false,
};

pub struct NsSpeechEngine {
    id: EngineId,
    worker_tx: mpsc::Sender<NsSpeechRequest>,
    voice_catalog: Arc<Vec<TtsVoice>>,
}

#[async_trait]
impl TtsEngine for NsSpeechEngine {
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
        if request.ssml {
            return Err(TtsError::SsmlUnsupported {
                id: self.id.clone(),
            });
        }

        if !self.voice_catalog.iter().any(|v| v.id == request.voice_id) {
            return Err(TtsError::InvalidVoice {
                id: request.voice_id,
            });
        }

        let (oneshot_tx, oneshot_rx) = tokio::sync::oneshot::channel();
        self.worker_tx
            .send(NsSpeechRequest::Synthesize {
                voice_id: request.voice_id.clone(),
                req: request,
                tx: oneshot_tx,
            })
            .map_err(|_| TtsError::EngineUnavailable {
                id: self.id.clone(),
                detail: "AVFoundation worker terminated".into(),
            })?;

        oneshot_rx
            .await
            .map_err(|_| TtsError::EngineUnavailable {
                id: self.id.clone(),
                detail: "AVFoundation worker dropped request".into(),
            })?
            .map_err(TtsError::from)
    }
}

pub struct NsSpeechEngineFactory;

impl TtsEngineFactory for NsSpeechEngineFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        let engine_id = EngineId("nsspeech".into());
        let catalog = voices::voice_catalog(&engine_id).map_err(TtsError::from)?;
        let worker_tx = spawn_worker();

        tracing::info!(voices = catalog.len(), "AVFoundation TTS engine ready");
        Ok(Box::new(NsSpeechEngine {
            id: engine_id,
            worker_tx,
            voice_catalog: Arc::new(catalog),
        }))
    }
}
