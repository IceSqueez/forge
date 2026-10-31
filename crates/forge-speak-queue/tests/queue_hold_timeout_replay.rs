#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_speak_queue::{QueueConfig, RequestId, SpeakCommand, SpeakEvent, SpeakEventStream};
use forge_speech_types::AssignmentStrategy;
use forge_speech_types::{EngineId, PcmBuffer, TtsVoice};
use forge_tts_core::{
    EngineCapabilities, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError, TtsRegistry,
};
use tokio::sync::Notify;

use common::{
    RecordingPublisher, assert_no_event_once_settled_on_paused_clock, make_deps, next_matching,
    recording_bus, recording_sink, request, voice, wait_for,
};

const ENGINE: &str = "gated";
const HANGING_TEXT: &str = "this synthesis never returns";
const SHORT_SYNTHESIS_TIMEOUT: Duration = Duration::from_millis(50);

#[derive(Default)]
struct Gate {
    entered: Notify,
    release: Notify,
}

struct GatedEngine {
    id: EngineId,
    gate: Option<Arc<Gate>>,
}

#[async_trait]
impl TtsEngine for GatedEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }
    fn capabilities(&self) -> &EngineCapabilities {
        &common::PLAIN_CAPABILITIES
    }
    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        Ok(vec![voice("gated-voice", ENGINE)])
    }
    async fn synthesize(&self, req: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        if req.text == HANGING_TEXT {
            std::future::pending::<()>().await;
        }
        if let Some(gate) = &self.gate {
            gate.entered.notify_one();
            gate.release.notified().await;
        }
        Ok(PcmBuffer::new(vec![0i16; 4], 22_050, 1))
    }
}

struct GatedFactory {
    gate: Option<Arc<Gate>>,
}

impl TtsEngineFactory for GatedFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Local
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(GatedEngine {
            id: EngineId(ENGINE.into()),
            gate: self.gate.clone(),
        }))
    }
}

fn config() -> QueueConfig {
    QueueConfig {
        per_user_limit: 10,
        max_queue_len: 50,
        ..QueueConfig::default()
    }
}

fn is_started_with_a_voice(ev: &SpeakEvent) -> bool {
    matches!(ev, SpeakEvent::Started { voice_id, .. } if voice_id.is_some())
}

fn is_finished(ev: &SpeakEvent, id: &RequestId) -> bool {
    matches!(ev, SpeakEvent::Finished { request_id } if request_id == id)
}

struct Harness {
    handle: forge_speak_queue::SpeakQueueHandle,
    stream: SpeakEventStream,
    plays: Arc<Mutex<usize>>,
    bus: Arc<RecordingPublisher>,
}

impl Harness {
    fn plays(&self) -> usize {
        *self.plays.lock().unwrap()
    }
}

fn spawn_queue(config: QueueConfig, gate: Option<Arc<Gate>>) -> Harness {
    let (sink, plays) = recording_sink();
    let mut registry = TtsRegistry::new();
    registry.register(EngineId(ENGINE.into()), Arc::new(GatedFactory { gate }));
    let (deps, bus) = recording_bus(make_deps(
        registry,
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    ));
    let (handle, stream) = forge_speak_queue::spawn(config, deps);
    Harness {
        handle,
        stream,
        plays,
        bus,
    }
}

async fn paused_mid_synthesis(q: &mut Harness, gate: &Gate, text: &str) -> RequestId {
    let req = request("viewer", text);
    let id = req.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    gate.entered.notified().await;
    q.handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut q.stream, |e| matches!(e, SpeakEvent::Paused)).await;
    gate.release.notify_one();
    id
}

#[tokio::test(start_paused = true)]
async fn a_clip_synthesized_while_paused_is_held_until_resume() {
    let gate = Arc::new(Gate::default());
    let mut q = spawn_queue(config(), Some(Arc::clone(&gate)));
    let id = paused_mid_synthesis(&mut q, &gate, "sponsor read in progress").await;

    assert_no_event_once_settled_on_paused_clock(&mut q.stream, |e| {
        is_started_with_a_voice(e) || is_finished(e, &id)
    })
    .await;
    assert_eq!(q.plays(), 0, "the sink ran while the queue was paused");

    q.handle.send(SpeakCommand::Resume).await.unwrap();
    wait_for(&mut q.stream, |e| is_finished(e, &id)).await;
    assert_eq!(q.plays(), 1, "resume must play the held clip once");
}

#[tokio::test(start_paused = true)]
async fn skipping_a_held_clip_drops_it_and_resume_plays_the_next_message() {
    let gate = Arc::new(Gate::default());
    let mut q = spawn_queue(config(), Some(Arc::clone(&gate)));
    let held = paused_mid_synthesis(&mut q, &gate, "held then skipped").await;
    assert_no_event_once_settled_on_paused_clock(&mut q.stream, is_started_with_a_voice).await;

    q.handle.send(SpeakCommand::Skip).await.unwrap();
    wait_for(
        &mut q.stream,
        |e| matches!(e, SpeakEvent::Skipped { request_id, .. } if *request_id == held),
    )
    .await;
    let next = request("viewer", "the next message");
    let next_id = next.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(next)).await.unwrap();
    q.handle.send(SpeakCommand::Resume).await.unwrap();
    gate.entered.notified().await;
    gate.release.notify_one();
    wait_for(&mut q.stream, |e| is_finished(e, &next_id)).await;

    assert_eq!(
        q.bus
            .payload_of("speak.skipped", &held)
            .map(|p| p["reason"].clone()),
        Some(serde_json::json!("user_skip"))
    );
    assert_eq!(
        q.plays(),
        1,
        "only the next message may reach the sink, never the skipped held clip"
    );
}

#[tokio::test(start_paused = true)]
async fn a_synthesis_past_timeout_per_item_fails_as_engine_timeout_and_the_queue_moves_on() {
    let mut q = spawn_queue(
        QueueConfig {
            timeout_per_item: SHORT_SYNTHESIS_TIMEOUT,
            ..config()
        },
        None,
    );
    let stuck = request("viewer", HANGING_TEXT);
    let stuck_id = stuck.request_id.clone();
    let next = request("viewer", "spoken after the stuck one");
    let next_id = next.request_id.clone();

    q.handle.send(SpeakCommand::Enqueue(stuck)).await.unwrap();
    q.handle.send(SpeakCommand::Enqueue(next)).await.unwrap();
    wait_for(&mut q.stream, |e| is_finished(e, &next_id)).await;

    assert_eq!(
        q.bus
            .payload_of("speak.failed", &stuck_id)
            .map(|p| p["reason"].clone()),
        Some(serde_json::json!("engine_timeout"))
    );
    assert_eq!(q.plays(), 1, "only the message after the stuck one played");
}

async fn speak_once(q: &mut Harness, viewer: &str) {
    let req = request(viewer, "the message a replay repeats");
    let id = req.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    wait_for(&mut q.stream, |e| is_finished(e, &id)).await;
    q.handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut q.stream, |e| matches!(e, SpeakEvent::Paused)).await;
}

async fn enqueue_admitted(q: &mut Harness, viewer: &str) {
    let req = request(viewer, "already waiting");
    let id = req.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    wait_for(
        &mut q.stream,
        |e| matches!(e, SpeakEvent::Enqueued { request_id, .. } if *request_id == id),
    )
    .await;
}

async fn next_admission(q: &mut Harness) -> SpeakEvent {
    next_matching(&mut q.stream, |e| {
        matches!(e, SpeakEvent::Enqueued { .. } | SpeakEvent::Rejected { .. })
    })
    .await
}

#[tokio::test]
async fn a_replay_is_rejected_when_the_queue_is_full() {
    let mut q = spawn_queue(
        QueueConfig {
            max_queue_len: 1,
            ..config()
        },
        None,
    );
    speak_once(&mut q, "replayed-viewer").await;
    enqueue_admitted(&mut q, "someone-else").await;

    q.handle.send(SpeakCommand::Replay).await.unwrap();

    let admission = next_admission(&mut q).await;
    assert!(
        matches!(&admission, SpeakEvent::Rejected { message, .. } if message.code().to_string() == "speak.rejected_queue_full"),
        "a replay must not grow a full queue, got {admission:?}"
    );
}

#[tokio::test]
async fn a_replay_is_rejected_when_the_viewer_has_spent_the_per_user_limit() {
    let mut q = spawn_queue(
        QueueConfig {
            per_user_limit: 1,
            ..config()
        },
        None,
    );
    speak_once(&mut q, "replayed-viewer").await;
    enqueue_admitted(&mut q, "replayed-viewer").await;

    q.handle.send(SpeakCommand::Replay).await.unwrap();

    let admission = next_admission(&mut q).await;
    assert!(
        matches!(&admission, SpeakEvent::Rejected { message, .. } if message.code().to_string() == "speak.rejected_per_user_limit"),
        "a replay must respect the viewer's limit, got {admission:?}"
    );
}

#[tokio::test]
async fn an_admitted_replay_counts_against_the_viewers_limit() {
    let mut q = spawn_queue(
        QueueConfig {
            per_user_limit: 2,
            ..config()
        },
        None,
    );
    speak_once(&mut q, "replayed-viewer").await;
    enqueue_admitted(&mut q, "replayed-viewer").await;
    q.handle.send(SpeakCommand::Replay).await.unwrap();
    assert!(matches!(
        next_admission(&mut q).await,
        SpeakEvent::Enqueued { .. }
    ));

    q.handle
        .send(SpeakCommand::Enqueue(request(
            "replayed-viewer",
            "one too many",
        )))
        .await
        .unwrap();

    let admission = next_admission(&mut q).await;
    assert!(
        matches!(&admission, SpeakEvent::Rejected { message, .. } if message.code().to_string() == "speak.rejected_per_user_limit"),
        "the replay must have used one of the viewer's slots, got {admission:?}"
    );
}
