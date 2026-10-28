use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use forge_speech_types::{EngineId, TtsVoice};
use forge_tts_core::{EngineSlot, TtsRegistry};

use crate::SpeakRequest;

pub(crate) const ENGINE_CATALOG_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const CATALOG_UPDATE_CAPACITY: usize = 16;

pub(crate) struct CatalogUpdate {
    generation: u64,
    engine_id: EngineId,
    voices: Option<Vec<TtsVoice>>,
}

pub(crate) type CatalogUpdateSender = tokio::sync::mpsc::Sender<CatalogUpdate>;

#[derive(Default)]
pub(crate) struct VoiceCatalogState {
    generation: u64,
    expected: HashSet<EngineId>,
    settled: HashMap<EngineId, Vec<TtsVoice>>,
    startup_complete: bool,
}

impl VoiceCatalogState {
    pub(crate) fn rebuild(
        &mut self,
        registry: &std::sync::RwLock<TtsRegistry>,
        disabled: &HashSet<EngineId>,
        tx: &CatalogUpdateSender,
    ) -> Arc<Vec<TtsVoice>> {
        let slots: Vec<(EngineId, Arc<EngineSlot>)> = {
            let guard = registry.read().unwrap_or_else(|e| e.into_inner());
            guard
                .engine_ids()
                .into_iter()
                .filter(|engine_id| !disabled.contains(engine_id))
                .filter_map(|engine_id| guard.slot(&engine_id).map(|slot| (engine_id, slot)))
                .collect()
        };
        self.generation = self.generation.wrapping_add(1);
        self.expected = slots
            .iter()
            .map(|(engine_id, _)| engine_id.clone())
            .collect();
        let expected = &self.expected;
        self.settled
            .retain(|engine_id, _| expected.contains(engine_id));
        for (engine_id, slot) in slots {
            spawn_engine_listing(self.generation, engine_id, slot, tx.clone());
        }
        self.note_startup_progress();
        self.merged()
    }

    pub(crate) fn apply(&mut self, update: CatalogUpdate) -> Option<Arc<Vec<TtsVoice>>> {
        if update.generation != self.generation || !self.expected.contains(&update.engine_id) {
            return None;
        }
        match update.voices {
            Some(voices) => {
                self.settled.insert(update.engine_id, voices);
            }
            None => {
                self.settled.entry(update.engine_id).or_default();
            }
        }
        self.note_startup_progress();
        Some(self.merged())
    }

    pub(crate) fn serves(&self, request: &SpeakRequest, alias_engine: Option<&EngineId>) -> bool {
        if self.startup_complete {
            return true;
        }
        let settled = |engine_id: &EngineId| self.settled.contains_key(engine_id);
        request.engine_override.as_ref().is_some_and(settled)
            || alias_engine.is_some_and(settled)
            || self.all_settled()
    }

    fn all_settled(&self) -> bool {
        self.expected
            .iter()
            .all(|engine_id| self.settled.contains_key(engine_id))
    }

    fn note_startup_progress(&mut self) {
        if !self.startup_complete && self.all_settled() {
            self.startup_complete = true;
        }
    }

    fn merged(&self) -> Arc<Vec<TtsVoice>> {
        let mut engines: Vec<(&EngineId, &Vec<TtsVoice>)> = self.settled.iter().collect();
        engines.sort_by(|(a, _), (b, _)| a.0.cmp(&b.0));
        Arc::new(
            engines
                .into_iter()
                .flat_map(|(_, voices)| voices.iter().cloned())
                .collect(),
        )
    }
}

fn spawn_engine_listing(
    generation: u64,
    engine_id: EngineId,
    slot: Arc<EngineSlot>,
    tx: CatalogUpdateSender,
) {
    tokio::spawn(async move {
        let listing_id = engine_id.clone();
        let mut listing = tokio::spawn(async move { list_engine_voices(&listing_id, &slot).await });
        let voices = match tokio::time::timeout(ENGINE_CATALOG_TIMEOUT, &mut listing).await {
            Ok(Ok(voices)) => voices,
            Ok(Err(e)) => {
                tracing::warn!(engine = %engine_id.0, "voice listing task failed: {e}");
                None
            }
            Err(_) => {
                listing.abort();
                tracing::warn!(
                    engine = %engine_id.0,
                    timeout_secs = ENGINE_CATALOG_TIMEOUT.as_secs(),
                    "voice listing timed out"
                );
                None
            }
        };
        if tx
            .send(CatalogUpdate {
                generation,
                engine_id,
                voices,
            })
            .await
            .is_err()
        {
            tracing::debug!("speak queue stopped before the voice catalog arrived");
        }
    });
}

async fn list_engine_voices(engine_id: &EngineId, slot: &EngineSlot) -> Option<Vec<TtsVoice>> {
    match slot.engine().await {
        Ok(engine) => match engine.list_voices().await {
            Ok(voices) => Some(voices),
            Err(e) => {
                tracing::warn!(engine = %engine_id.0, "list_voices: {e}");
                None
            }
        },
        Err(e) => {
            tracing::warn!(engine = %engine_id.0, "create engine: {e}");
            None
        }
    }
}
