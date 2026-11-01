use std::sync::Arc;
use std::time::Duration;

use forge_tts_avspeech::AvSpeechEngineFactory;
use forge_tts_core::TtsEngineFactory;
use forge_tts_espeak::EspeakEngineFactory;
use forge_tts_piper::{PiperEngine, PiperEngineFactory};
use forge_tts_sapi::SapiEngineFactory;

use forge_host::speak_boot::{LocalTtsEngine, TtsEngineAdmission};

const PIPER_SYNTHESIS_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn discover_local_tts_engines() -> Vec<LocalTtsEngine> {
    let mut engines = Vec::new();
    if let Some(piper_binary) = PiperEngine::locate().await {
        let voices_dir = PiperEngine::voices_dir(&crate::app::app_paths::app_paths().data_dir());
        if let Err(e) = std::fs::create_dir_all(&voices_dir) {
            tracing::warn!(error = %e, path = %voices_dir.display(), "failed to create the piper voices dir");
        }
        tracing::info!(path = %piper_binary.display(), "registered the Piper TTS engine");
        engines.push(LocalTtsEngine {
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
    engines.push(probed(Arc::new(EspeakEngineFactory::default())));
    engines.push(probed(Arc::new(SapiEngineFactory)));
    engines.push(probed(Arc::new(AvSpeechEngineFactory)));
    engines
}

fn probed(factory: Arc<dyn TtsEngineFactory>) -> LocalTtsEngine {
    LocalTtsEngine {
        factory,
        admission: TtsEngineAdmission::Probed,
    }
}
