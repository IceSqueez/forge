use forge_speech_types::EngineId;
use forge_tts_core::{
    ENGINE_ID_SAPI, ENGINE_SYNTHESIS_BUDGET, EngineClass, SsmlInput, ThreadEngine,
    ThreadEngineSpec, TtsEngine, TtsEngineFactory, TtsError,
};

use crate::com::{open_sta_worker, synthesize_on_sta};

const STA_THREAD_NAME: &str = "sapi-sta";

pub struct SapiEngineFactory;

impl TtsEngineFactory for SapiEngineFactory {
    fn engine_id(&self) -> EngineId {
        EngineId(ENGINE_ID_SAPI.into())
    }

    fn class(&self) -> EngineClass {
        EngineClass::System
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        let engine_id = self.engine_id();
        let spec = ThreadEngineSpec {
            engine_id: engine_id.clone(),
            thread_name: STA_THREAD_NAME,
            ssml_input: SsmlInput::Accepted,
            synthesis_budget: ENGINE_SYNTHESIS_BUDGET,
        };
        let engine = ThreadEngine::spawn(
            spec,
            move || {
                let (worker, catalog) = open_sta_worker(&engine_id)?;
                tracing::info!(voices = catalog.len(), "SAPI 5 engine ready");
                Ok((worker, catalog))
            },
            |worker, request, watch| {
                synthesize_on_sta(worker, &request, watch).map_err(TtsError::from)
            },
        )?;
        Ok(Box::new(engine))
    }
}
