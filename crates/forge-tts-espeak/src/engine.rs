use std::path::PathBuf;

use async_trait::async_trait;
use forge_speech_types::{EngineId, PcmBuffer, TtsVoice};
use forge_tts_core::{
    ENGINE_ID_ESPEAK_NG, EngineCapabilities, SynthesisRequest, TtsEngine, TtsEngineFactory,
    TtsError,
};

use crate::{process, voices, wav};

static CAPABILITIES: EngineCapabilities = EngineCapabilities {
    ssml: true,
    neural_voices: false,
    streaming: false,
    custom_lexicons: false,
};

fn espeak_engine_id() -> EngineId {
    EngineId(ENGINE_ID_ESPEAK_NG.into())
}

pub struct EspeakEngine {
    id: EngineId,
    program: PathBuf,
    voice_cache: tokio::sync::Mutex<Option<Vec<TtsVoice>>>,
}

impl EspeakEngine {
    pub fn new(program: PathBuf) -> Result<Self, TtsError> {
        process::check_espeak_version(&program).map_err(TtsError::from)?;
        Ok(Self {
            id: espeak_engine_id(),
            program,
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
        let mut cache = self.voice_cache.lock().await;
        if let Some(cached) = cache.as_ref() {
            return Ok(cached.clone());
        }
        let raw = process::list_voices_from_binary(&self.program)
            .await
            .map_err(TtsError::from)?;
        let result = voices::parse_voices_output(&raw, &self.id);
        *cache = Some(result.clone());
        Ok(result)
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        {
            let mut cache = self.voice_cache.lock().await;
            if cache.is_none() {
                let raw = process::list_voices_from_binary(&self.program)
                    .await
                    .map_err(TtsError::from)?;
                *cache = Some(voices::parse_voices_output(&raw, &self.id));
            }
            let known = cache
                .as_ref()
                .is_some_and(|voices| voices.iter().any(|v| v.id == request.voice_id));
            if !known {
                return Err(TtsError::InvalidVoice {
                    id: request.voice_id,
                });
            }
        }

        let rate_wpm = process::rate_wpm_from_multiplier(request.rate_multiplier);
        let pitch_0_99 = process::pitch_from_semitones(request.pitch_semitones);

        let wav_bytes = process::run_synthesis(
            &self.program,
            &process::SynthesisInvocation {
                voice_id: &request.voice_id.0,
                text: &request.text,
                rate_wpm,
                pitch_0_99,
                ssml: request.ssml,
            },
        )
        .await
        .map_err(TtsError::from)?;

        let wav = wav::parse(&wav_bytes).map_err(TtsError::from)?;
        Ok(PcmBuffer::new(wav.samples, wav.sample_rate, wav.channels))
    }
}

pub struct EspeakEngineFactory {
    program: PathBuf,
}

impl EspeakEngineFactory {
    pub fn new(program: PathBuf) -> Self {
        Self { program }
    }
}

impl Default for EspeakEngineFactory {
    fn default() -> Self {
        Self::new(PathBuf::from(process::ESPEAK_BINARY))
    }
}

impl TtsEngineFactory for EspeakEngineFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Local
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        EspeakEngine::new(self.program.clone()).map(|e| Box::new(e) as Box<dyn TtsEngine>)
    }
}
