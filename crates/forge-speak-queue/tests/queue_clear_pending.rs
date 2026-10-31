mod common;

use std::sync::Arc;

use async_trait::async_trait;
use forge_speak_queue::{QueueConfig, SpeakCommand, SpeakEvent};
use forge_speech_types::AssignmentStrategy;
use forge_speech_types::PcmBuffer;
use forge_speech_types::{EngineId, TtsVoice};
use forge_tts_core::{
    EngineCapabilities, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError, TtsRegistry,
};
use tokio::sync::Notify;

use common::{
    assert_no_event_once_settled_on_paused_clock, make_deps, recording_sink, request, voice,
    wait_for,
};

struct GatedEngine {
    gate: Arc<Notify>,
}

#[async_trait]
impl TtsEngine for GatedEngine {
    fn engine_id(&self) -> &EngineId {
        static ID: std::sync::OnceLock<EngineId> = std::sync::OnceLock::new();
        ID.get_or_init(|| EngineId("gate".into()))
    }
    fn capabilities(&self) -> &EngineCapabilities {
        &common::PLAIN_CAPABILITIES
    }
    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        Ok(vec![voice("g-voice", "gate")])
    }
    async fn synthesize(&self, _req: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        self.gate.notified().await;
        Ok(PcmBuffer::new(vec![0i16; 8], 22_050, 1))
    }
}

struct GatedFactory {
    gate: Arc<Notify>,
}
impl TtsEngineFactory for GatedFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Local
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(GatedEngine {
            gate: self.gate.clone(),
        }))
    }
}

fn gated_registry(gate: Arc<Notify>) -> TtsRegistry {
    let mut reg = TtsRegistry::new();
    reg.register(EngineId("gate".into()), Arc::new(GatedFactory { gate }));
    reg
}

#[tokio::test]
async fn clear_pending_lets_the_in_flight_item_finish() {
    let gate = Arc::new(Notify::new());
    let (sink, plays) = recording_sink();
    let deps = make_deps(
        gated_registry(gate.clone()),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    handle
        .send(SpeakCommand::Enqueue(request("v1", "in flight")))
        .await
        .unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Started { .. })).await;

    handle.send(SpeakCommand::ClearPending).await.unwrap();
    wait_for(&mut stream, |e| {
        matches!(e, SpeakEvent::QueueChanged { queue_len: 0, .. })
    })
    .await;

    gate.notify_one();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Finished { .. })).await;
    assert_eq!(*plays.lock().unwrap(), 1, "in-flight item must still play");
}

#[tokio::test(start_paused = true)]
async fn clear_abandons_the_in_flight_item() {
    let gate = Arc::new(Notify::new());
    let (sink, plays) = recording_sink();
    let deps = make_deps(
        gated_registry(gate.clone()),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    handle
        .send(SpeakCommand::Enqueue(request("v1", "in flight")))
        .await
        .unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Started { .. })).await;

    handle.send(SpeakCommand::Clear).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Cleared)).await;

    gate.notify_one();
    assert_no_event_once_settled_on_paused_clock(&mut stream, |e| {
        matches!(e, SpeakEvent::Finished { .. })
    })
    .await;
    assert_eq!(*plays.lock().unwrap(), 0, "abandoned item must not play");
}
