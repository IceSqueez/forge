use std::sync::Arc;

use forge_net::TaskOwner;
use forge_speech_types::TtsVoice;
use forge_tts_pipeline::LanguageDetector;

use crate::voice::candidate_languages;

pub(crate) struct DetectorBuild {
    pub(crate) generation: u64,
    pub(crate) detector: Option<Arc<LanguageDetector>>,
}

pub(crate) struct DetectorBuilds {
    pub(crate) generation: u64,
    tx: tokio::sync::mpsc::Sender<DetectorBuild>,
    ready: tokio::sync::watch::Sender<bool>,
    tasks: TaskOwner,
}

impl DetectorBuilds {
    pub(crate) fn new(
        tx: tokio::sync::mpsc::Sender<DetectorBuild>,
        ready: tokio::sync::watch::Sender<bool>,
        tasks: TaskOwner,
    ) -> Self {
        Self {
            generation: 0,
            tx,
            ready,
            tasks,
        }
    }

    pub(crate) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.ready.send_replace(false);
    }

    pub(crate) fn settle(&self, ready: bool) {
        self.ready.send_replace(ready);
    }

    pub(crate) fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }

    pub(crate) fn rebuild(&mut self, catalog: &[TtsVoice]) {
        self.invalidate();
        let generation = self.generation;
        let candidates = candidate_languages(catalog);
        let tx = self.tx.clone();
        self.tasks.spawn_until_stopped(async move {
            let built = tokio::task::spawn_blocking(move || {
                LanguageDetector::new(&candidates).map(Arc::new)
            })
            .await;
            let detector = match built {
                Ok(detector) => detector,
                Err(e) => {
                    tracing::warn!(error = %e, "language detector build failed");
                    None
                }
            };
            if tx
                .send(DetectorBuild {
                    generation,
                    detector,
                })
                .await
                .is_err()
            {
                tracing::debug!("speak queue stopped before the language detector was rebuilt");
            }
        });
    }
}
