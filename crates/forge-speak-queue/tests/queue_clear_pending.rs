//! The load-bearing distinction between `ClearPending` and `Clear`:
//!
//! - `ClearPending` drops pending items but lets the IN-FLIGHT item finish.
//! - `Clear` also abandons the active item.
//!
//! Both are driven with a gated synthesis engine so exactly one item is in-flight
//! when the command lands; the tests assert they DIFFER on the active item.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use forge_audio::PcmBuffer;
use forge_speak_queue::{QueueConfig, SpeakCommand, SpeakEvent};
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError,
    TtsRegistry, TtsVoice,
};
use forge_voice::AssignmentStrategy;
use tokio::sync::Notify;

use common::{assert_no_event, make_deps, recording_sink, request, voice, wait_for};

/// Engine whose `synthesize` blocks on a gate, pinning one item in-flight until the
/// test releases it — `list_voices` stays unblocked so the catalog still builds.
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
        static CAPS: EngineCapabilities = EngineCapabilities {
            ssml: false,
            neural_voices: false,
            streaming: false,
            custom_lexicons: false,
        };
        &CAPS
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
    // First Started (empty ids) confirms the item is active and synth is gated.
    wait_for(
        &mut stream,
        |e| matches!(e, SpeakEvent::Started { .. }),
        2_000,
    )
    .await;

    handle.send(SpeakCommand::ClearPending).await.unwrap();
    // QueueChanged{0} is ClearPending's processed barrier.
    wait_for(
        &mut stream,
        |e| matches!(e, SpeakEvent::QueueChanged { queue_len: 0 }),
        2_000,
    )
    .await;

    // Release synthesis; the active item must complete despite ClearPending.
    gate.notify_one();
    wait_for(
        &mut stream,
        |e| matches!(e, SpeakEvent::Finished { .. }),
        2_000,
    )
    .await;
    assert_eq!(*plays.lock().unwrap(), 1, "in-flight item must still play");
}

#[tokio::test]
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
    wait_for(
        &mut stream,
        |e| matches!(e, SpeakEvent::Started { .. }),
        2_000,
    )
    .await;

    handle.send(SpeakCommand::Clear).await.unwrap();
    // Cleared is Clear's processed barrier; it clears active_request_id.
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Cleared), 2_000).await;

    // Release synthesis; the synth result must be discarded — no playback, no Finished.
    gate.notify_one();
    assert_no_event(
        &mut stream,
        |e| matches!(e, SpeakEvent::Finished { .. }),
        300,
    )
    .await;
    assert_eq!(*plays.lock().unwrap(), 0, "abandoned item must not play");
}
