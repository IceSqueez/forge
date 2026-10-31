#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use forge_speak_queue::{
    QueueConfig, QueueDeps, RequestId, SpeakCommand, SpeakEvent, SpeakEventStream, SpeakQueueHandle,
};
use forge_speech_types::PcmBuffer;
use forge_speech_types::{AssignmentStrategy, SynthesisDefaults};
use forge_speech_types::{EngineId, TtsVoice};
use forge_tts_core::{
    EngineCapabilities, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError, TtsRegistry,
};

use common::{HANG_GUARD, make_deps, outcome_of, recording_sink, request_with_overrides, voice};

struct GenerationEngine {
    id: EngineId,
    generation: usize,
    spoken_by: Arc<std::sync::Mutex<Vec<usize>>>,
    gate: Option<Arc<tokio::sync::Semaphore>>,
    entered: tokio::sync::mpsc::UnboundedSender<usize>,
}

#[async_trait]
impl TtsEngine for GenerationEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }
    fn capabilities(&self) -> &EngineCapabilities {
        &common::PLAIN_CAPABILITIES
    }
    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        Ok(vec![voice(&format!("{}-1", self.id.0), &self.id.0)])
    }
    async fn synthesize(&self, _req: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the receiver may already be gone"
        )]
        let _ = self.entered.send(self.generation);
        if let Some(gate) = &self.gate {
            let _permit = gate.acquire().await.expect("gate open");
        }
        self.spoken_by.lock().unwrap().push(self.generation);
        Ok(PcmBuffer::new(vec![0i16; 8], 22_050, 1))
    }
}

struct CountingFactory {
    id: EngineId,
    creates: AtomicUsize,
    created: tokio::sync::mpsc::UnboundedSender<usize>,
    spoken_by: Arc<std::sync::Mutex<Vec<usize>>>,
    gate: Option<Arc<tokio::sync::Semaphore>>,
    entered: tokio::sync::mpsc::UnboundedSender<usize>,
}

impl TtsEngineFactory for CountingFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Local
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        let generation = self.creates.fetch_add(1, Ordering::SeqCst) + 1;
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the receiver may already be gone"
        )]
        let _ = self.created.send(generation);
        Ok(Box::new(GenerationEngine {
            id: self.id.clone(),
            generation,
            spoken_by: self.spoken_by.clone(),
            gate: self.gate.clone(),
            entered: self.entered.clone(),
        }))
    }
}

struct Probe {
    factory: Arc<CountingFactory>,
    created: tokio::sync::mpsc::UnboundedReceiver<usize>,
    entered: tokio::sync::mpsc::UnboundedReceiver<usize>,
    spoken_by: Arc<std::sync::Mutex<Vec<usize>>>,
}

impl Probe {
    fn new(engine: &str, gate: Option<Arc<tokio::sync::Semaphore>>) -> Self {
        let (created_tx, created) = tokio::sync::mpsc::unbounded_channel();
        let (entered_tx, entered) = tokio::sync::mpsc::unbounded_channel();
        let spoken_by = Arc::new(std::sync::Mutex::new(Vec::new()));
        Self {
            factory: Arc::new(CountingFactory {
                id: EngineId(engine.into()),
                creates: AtomicUsize::new(0),
                created: created_tx,
                spoken_by: spoken_by.clone(),
                gate,
                entered: entered_tx,
            }),
            created,
            entered,
            spoken_by,
        }
    }

    fn creates(&self) -> usize {
        self.factory.creates.load(Ordering::SeqCst)
    }

    async fn next_build(&mut self) -> usize {
        tokio::time::timeout(HANG_GUARD, self.created.recv())
            .await
            .expect("hang guard: no engine build arrived")
            .expect("factory alive")
    }
}

fn registry_of(probes: &[&Probe]) -> TtsRegistry {
    let mut registry = TtsRegistry::new();
    for probe in probes {
        registry.register(probe.factory.id.clone(), probe.factory.clone());
    }
    registry
}

fn deps_for(registry: TtsRegistry) -> QueueDeps {
    let (sink, _plays) = recording_sink();
    make_deps(
        registry,
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    )
}

fn start(deps: QueueDeps) -> (SpeakQueueHandle, SpeakEventStream) {
    forge_speak_queue::spawn(QueueConfig::default(), deps)
}

async fn enqueue_on(handle: &SpeakQueueHandle, engine: &str) -> RequestId {
    let voice_id = format!("{engine}-1");
    let req = request_with_overrides("viewer", Some(engine), Some(&voice_id));
    let id = req.request_id.clone();
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    id
}

async fn speak_on(handle: &SpeakQueueHandle, stream: &mut SpeakEventStream, engine: &str) {
    let id = enqueue_on(handle, engine).await;
    let outcome = outcome_of(stream, &id).await;
    assert!(
        matches!(outcome, SpeakEvent::Finished { .. }),
        "a request on {engine} did not finish: {outcome:?}"
    );
}

#[tokio::test]
async fn a_run_of_messages_builds_each_engine_once() {
    let alpha = Probe::new("alpha", None);
    let beta = Probe::new("beta", None);
    let (handle, mut stream) = start(deps_for(registry_of(&[&alpha, &beta])));

    for engine in ["alpha", "beta", "alpha", "alpha", "beta"] {
        speak_on(&handle, &mut stream, engine).await;
    }

    assert_eq!((alpha.creates(), beta.creates()), (1, 1));
}

#[tokio::test]
async fn resending_an_engines_current_enabled_state_rebuilds_nothing() {
    let alpha = Probe::new("alpha", None);
    let (handle, mut stream) = start(deps_for(registry_of(&[&alpha])));
    speak_on(&handle, &mut stream, "alpha").await;

    handle
        .send(SpeakCommand::SetEngineEnabled(
            EngineId("alpha".into()),
            true,
        ))
        .await
        .unwrap();
    speak_on(&handle, &mut stream, "alpha").await;
    speak_on(&handle, &mut stream, "alpha").await;

    assert_eq!(alpha.creates(), 1);
}

#[tokio::test]
async fn flipping_an_engine_off_and_on_rebuilds_it_once_on_next_use() {
    let alpha = Probe::new("alpha", None);
    let (handle, mut stream) = start(deps_for(registry_of(&[&alpha])));
    speak_on(&handle, &mut stream, "alpha").await;

    for enabled in [false, true] {
        handle
            .send(SpeakCommand::SetEngineEnabled(
                EngineId("alpha".into()),
                enabled,
            ))
            .await
            .unwrap();
    }
    speak_on(&handle, &mut stream, "alpha").await;
    speak_on(&handle, &mut stream, "alpha").await;

    assert_eq!(alpha.creates(), 2);
}

#[tokio::test]
async fn refreshing_the_voice_catalog_rebuilds_every_engine_once() {
    let alpha = Probe::new("alpha", None);
    let beta = Probe::new("beta", None);
    let (handle, mut stream) = start(deps_for(registry_of(&[&alpha, &beta])));
    speak_on(&handle, &mut stream, "alpha").await;
    speak_on(&handle, &mut stream, "beta").await;

    handle
        .send(SpeakCommand::RefreshVoiceCatalog)
        .await
        .unwrap();
    for engine in ["alpha", "alpha", "beta", "beta"] {
        speak_on(&handle, &mut stream, engine).await;
    }

    assert_eq!((alpha.creates(), beta.creates()), (2, 2));
}

#[tokio::test]
async fn changing_engine_params_keeps_the_built_engine() {
    let alpha = Probe::new("alpha", None);
    let (handle, mut stream) = start(deps_for(registry_of(&[&alpha])));
    speak_on(&handle, &mut stream, "alpha").await;

    handle
        .send(SpeakCommand::SetEngineParams(
            EngineId("alpha".into()),
            SynthesisDefaults {
                pitch_semitones: 3.0,
                rate_multiplier: 1.5,
            },
            0.5,
        ))
        .await
        .unwrap();
    speak_on(&handle, &mut stream, "alpha").await;
    speak_on(&handle, &mut stream, "alpha").await;

    assert_eq!(alpha.creates(), 1);
}

#[tokio::test]
async fn a_message_in_flight_during_a_catalog_refresh_finishes_on_the_old_engine() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let mut alpha = Probe::new("alpha", Some(gate.clone()));
    let (handle, mut stream) = start(deps_for(registry_of(&[&alpha])));
    let boot_build = alpha.next_build().await;

    let in_flight = enqueue_on(&handle, "alpha").await;
    let speaking_on = tokio::time::timeout(HANG_GUARD, alpha.entered.recv())
        .await
        .expect("hang guard: synthesis never started")
        .expect("engine alive");
    handle
        .send(SpeakCommand::RefreshVoiceCatalog)
        .await
        .unwrap();
    let rebuild = alpha.next_build().await;
    gate.add_permits(1);
    let outcome = outcome_of(&mut stream, &in_flight).await;
    speak_on(&handle, &mut stream, "alpha").await;

    assert!(
        matches!(outcome, SpeakEvent::Finished { .. }),
        "{outcome:?}"
    );
    assert_eq!((boot_build, speaking_on, rebuild), (1, 1, 2));
    assert_eq!(*alpha.spoken_by.lock().unwrap(), vec![1, 2]);
}

#[tokio::test]
async fn an_engine_disabled_at_boot_releases_its_probe_instance() {
    let alpha = Probe::new("alpha", None);
    let probe_instance: Arc<dyn TtsEngine> = Arc::new(GenerationEngine {
        id: EngineId("alpha".into()),
        generation: 0,
        spoken_by: alpha.spoken_by.clone(),
        gate: None,
        entered: alpha.factory.entered.clone(),
    });
    let released = Arc::downgrade(&probe_instance);
    let mut registry = TtsRegistry::new();
    registry.register_built(
        EngineId("alpha".into()),
        alpha.factory.clone(),
        probe_instance,
    );
    let deps = QueueDeps {
        disabled_engines: HashSet::from([EngineId("alpha".into())]),
        engine_gains: HashMap::new(),
        ..deps_for(registry)
    };
    let (handle, mut stream) = start(deps);

    handle.send(SpeakCommand::Pause).await.unwrap();
    common::wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused)).await;

    assert!(
        released.upgrade().is_none(),
        "the probe instance is still held"
    );
    assert_eq!(alpha.creates(), 0);
}
