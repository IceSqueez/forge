use forge_speech_types::EngineId;
use forge_tts_core::{
    ENGINE_ID_AVSPEECH_STORED_AS_NSSPEECH, ENGINE_SYNTHESIS_BUDGET, EngineClass, SsmlInput,
    ThreadEngine, ThreadEngineSpec, TtsEngine, TtsEngineFactory, TtsError,
};

use crate::synth::{new_synthesizer, run_synthesis};
use crate::voices;

const WORKER_THREAD_NAME: &str = "avspeech-worker";

pub struct AvSpeechEngineFactory;

impl TtsEngineFactory for AvSpeechEngineFactory {
    fn engine_id(&self) -> EngineId {
        EngineId(ENGINE_ID_AVSPEECH_STORED_AS_NSSPEECH.into())
    }

    fn class(&self) -> EngineClass {
        EngineClass::System
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        let engine_id = self.engine_id();
        let spec = ThreadEngineSpec {
            engine_id: engine_id.clone(),
            thread_name: WORKER_THREAD_NAME,
            ssml_input: SsmlInput::Refused,
            synthesis_budget: ENGINE_SYNTHESIS_BUDGET,
        };
        let engine = ThreadEngine::spawn(
            spec,
            move || {
                let catalog = voices::voice_catalog(&engine_id)?;
                tracing::info!(voices = catalog.len(), "AVSpeech engine ready");
                Ok((new_synthesizer(), catalog))
            },
            |synth, request, watch| run_synthesis(synth, request, watch).map_err(TtsError::from),
        )?;
        Ok(Box::new(engine))
    }
}
