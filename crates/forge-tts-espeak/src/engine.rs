use async_trait::async_trait;
use forge_tts_core::{
    EngineCapabilities, EngineId, PcmBuffer, SynthesisRequest, TtsEngine, TtsEngineFactory,
    TtsError, TtsVoice,
};

use crate::{process, voices};

static CAPABILITIES: EngineCapabilities = EngineCapabilities {
    ssml: true,
    neural_voices: false,
    streaming: false,
    custom_lexicons: true,
};

fn espeak_engine_id() -> EngineId {
    EngineId("espeak-ng".into())
}

pub struct EspeakEngine {
    id: EngineId,
    voice_cache: tokio::sync::Mutex<Option<Vec<TtsVoice>>>,
}

impl EspeakEngine {
    pub fn new() -> Result<Self, TtsError> {
        process::check_espeak_version().map_err(TtsError::from)?;
        Ok(Self {
            id: espeak_engine_id(),
            voice_cache: tokio::sync::Mutex::new(None),
        })
    }
}

#[async_trait]
impl TtsEngine for EspeakEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }

    fn capabilities(&self) -> &EngineCapabilities {
        &CAPABILITIES
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        {
            let cache = self.voice_cache.lock().await;
            if let Some(cached) = cache.as_ref() {
                return Ok(cached.clone());
            }
        }

        let raw = process::list_voices_from_binary()
            .await
            .map_err(TtsError::from)?;
        let result = voices::parse_voices_output(&raw, &self.id);

        {
            let mut cache = self.voice_cache.lock().await;
            *cache = Some(result.clone());
        }

        Ok(result)
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        let voices = self.list_voices().await?;

        if !voices.iter().any(|v| v.id == request.voice_id) {
            return Err(TtsError::InvalidVoice {
                id: request.voice_id,
            });
        }

        let rate_wpm = process::rate_wpm_from_multiplier(request.rate_multiplier);
        let pitch_0_99 = process::pitch_from_semitones(request.pitch_semitones);

        let raw_bytes = process::run_synthesis(
            &request.voice_id.0,
            &request.text,
            rate_wpm,
            pitch_0_99,
            request.ssml,
        )
        .await
        .map_err(TtsError::from)?;

        let samples: Vec<i16> = raw_bytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();

        Ok(PcmBuffer::new(samples, 22_050, 1))
    }
}

pub struct EspeakEngineFactory;

impl TtsEngineFactory for EspeakEngineFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        EspeakEngine::new().map(|e| Box::new(e) as Box<dyn TtsEngine>)
    }
}
