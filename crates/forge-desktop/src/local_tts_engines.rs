use std::sync::Arc;
use std::time::Duration;

use forge_speech_types::EngineId;
use forge_tts_core::TtsEngineFactory;
use forge_tts_espeak::EspeakEngineFactory;
use forge_tts_nsspeech::NsSpeechEngineFactory;
use forge_tts_piper::{PiperEngine, PiperEngineFactory};
use forge_tts_sapi::SapiEngineFactory;

use crate::speak_boot::{LocalTtsEngine, TtsEngineAdmission};

const PIPER_SYNTHESIS_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn discover_local_tts_engines() -> Vec<LocalTtsEngine> {
    let mut engines = Vec::new();
    if let Some(piper_binary) = PiperEngine::locate().await {
        let voices_dir = PiperEngine::voices_dir(&crate::app_paths::app_paths().data_dir());
        if let Err(e) = std::fs::create_dir_all(&voices_dir) {
            tracing::warn!(error = %e, path = %voices_dir.display(), "failed to create the piper voices dir");
        }
        tracing::info!(path = %piper_binary.display(), "registered the Piper TTS engine");
        engines.push(LocalTtsEngine {
            id: EngineId(forge_tts_core::ENGINE_ID_PIPER.into()),
            factory: Arc::new(PiperEngineFactory {
                piper_binary,
                voices_dir,
                timeout: PIPER_SYNTHESIS_TIMEOUT,
            }),
            admission: TtsEngineAdmission::Unprobed,
        });
    } else {
        tracing::info!(
            "Piper binary not found next to the executable or on PATH; Piper TTS disabled"
        );
    }
    engines.push(probed(
        forge_tts_core::ENGINE_ID_ESPEAK_NG,
        Arc::new(EspeakEngineFactory::default()),
    ));
    engines.push(probed(
        forge_tts_core::ENGINE_ID_SAPI,
        Arc::new(SapiEngineFactory),
    ));
    engines.push(probed(
        forge_tts_core::ENGINE_ID_NSSPEECH,
        Arc::new(NsSpeechEngineFactory),
    ));
    engines
}

fn probed(engine_id: &str, factory: Arc<dyn TtsEngineFactory>) -> LocalTtsEngine {
    LocalTtsEngine {
        id: EngineId(engine_id.into()),
        factory,
        admission: TtsEngineAdmission::Probed,
    }
}
