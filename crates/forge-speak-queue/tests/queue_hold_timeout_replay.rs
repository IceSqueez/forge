#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink, PcmBuffer};
use forge_events::{Event, EventPublisher};
use forge_speak_queue::{
    Priority, QueueConfig, QueueDeps, RequestId, SpeakCommand, SpeakEvent, SpeakEventStream,
    SpeakRequest,
};
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError,
    TtsRegistry, TtsVoice, VoiceGender, VoiceId,
};
use forge_types::EventId;
use forge_voice::{AssignmentStrategy, IgnoreProfile, SynthesisDefaults, VoiceAliasResolver};
use tokio::sync::Notify;

const ENGINE: &str = "gated";
const HANGING_TEXT: &str = "this synthesis never returns";
const EVENT_WAIT_MS: u64 = 2_000;
const QUIET_WINDOW_MS: u64 = 50;
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
        static CAPS: EngineCapabilities = EngineCapabilities {
            ssml: false,
            neural_voices: false,
            streaming: false,
            custom_lexicons: false,
        };
        &CAPS
    }
    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        Ok(vec![TtsVoice {
            id: VoiceId("gated-voice".into()),
            name: "Gated".into(),
            locale: "en-US".into(),
            gender: VoiceGender::Neutral,
            engine_id: EngineId(ENGINE.into()),
            is_neural: false,
            sample_rate_hint: 22_050,
        }])
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
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(GatedEngine {
            id: EngineId(ENGINE.into()),
            gate: self.gate.clone(),
        }))
    }
}

#[derive(Default)]
struct CountingSink {
    plays: Mutex<usize>,
}

impl CountingSink {
    fn plays(&self) -> usize {
        *self.plays.lock().unwrap()
    }
}

#[async_trait]
impl AudioSink for CountingSink {
    async fn play(&self, _buf: PcmBuffer) -> Result<(), AudioError> {
        *self.plays.lock().unwrap() += 1;
        Ok(())
    }
}

#[derive(Default)]
struct RecordingPublisher {
    events: Mutex<Vec<Event>>,
}

impl EventPublisher for RecordingPublisher {
    fn publish(&self, event: Event) {
        self.events.lock().unwrap().push(event);
    }
}

impl RecordingPublisher {
    fn published(&self, kind: &str, request_id: &RequestId) -> Option<serde_json::Value> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.kind == kind && e.payload["request_id"] == request_id.0.as_str())
            .map(|e| e.payload.clone())
    }
}

fn make_deps(
    gate: Option<Arc<Gate>>,
    sink: Arc<dyn AudioSink>,
    bus: Arc<RecordingPublisher>,
) -> QueueDeps {
    let mut registry = TtsRegistry::new();
    registry.register(EngineId(ENGINE.into()), Arc::new(GatedFactory { gate }));
    let resolver = VoiceAliasResolver::new(
        vec![],
        AssignmentStrategy::DeterministicByName,
        IgnoreProfile::default(),
        SynthesisDefaults::default(),
    );
    QueueDeps {
        registry: Arc::new(std::sync::RwLock::new(registry)),
        resolver: Arc::new(std::sync::RwLock::new(resolver)),
        pipeline: forge_speak_queue::PipelineConfigHandle::new(
            forge_tts_pipeline::PipelineConfig::default(),
        ),
        audio_sink: sink,
        event_bus: bus,
        disabled_engines: std::collections::HashSet::new(),
        engine_gains: std::collections::HashMap::new(),
    }
}

fn speak_req(viewer: &str, text: &str) -> SpeakRequest {
    SpeakRequest {
        request_id: RequestId::new(),
        viewer_id: viewer.into(),
        viewer_name: viewer.into(),
        text: text.into(),
        priority: Priority::Normal,
        alias_override: None,
        engine_override: None,
        voice_override: None,
        source_event_id: Some(EventId::new()),
        is_reward: false,
        target: None,
    }
}

fn config() -> QueueConfig {
    QueueConfig {
        per_user_limit: 10,
        max_queue_len: 50,
        ..QueueConfig::default()
    }
}

async fn wait_for<F>(stream: &mut SpeakEventStream, pred: F) -> SpeakEvent
where
    F: Fn(&SpeakEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_millis(EVENT_WAIT_MS);
    loop {
        match tokio::time::timeout_at(deadline, stream.recv()).await {
            Ok(Ok(ev)) if pred(&ev) => return ev,
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => panic!("stream closed"),
            Err(_) => panic!("timeout waiting for expected SpeakEvent"),
        }
    }
}

async fn assert_quiet<F>(stream: &mut SpeakEventStream, pred: F, what: &str)
where
    F: Fn(&SpeakEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_millis(QUIET_WINDOW_MS);
    while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, stream.recv()).await {
        assert!(!pred(&ev), "{what}: got {ev:?}");
    }
}

fn is_started_with_a_voice(ev: &SpeakEvent) -> bool {
    matches!(ev, SpeakEvent::Started { voice_id, .. } if !voice_id.0.is_empty())
}

fn is_finished(ev: &SpeakEvent, id: &RequestId) -> bool {
    matches!(ev, SpeakEvent::Finished { request_id } if request_id == id)
}

struct Harness {
    handle: forge_speak_queue::SpeakQueueHandle,
    stream: SpeakEventStream,
    sink: Arc<CountingSink>,
    bus: Arc<RecordingPublisher>,
}

fn spawn_queue(config: QueueConfig, gate: Option<Arc<Gate>>) -> Harness {
    let sink = Arc::new(CountingSink::default());
    let bus = Arc::new(RecordingPublisher::default());
    let (handle, stream) =
        forge_speak_queue::spawn(config, make_deps(gate, sink.clone(), Arc::clone(&bus)));
    Harness {
        handle,
        stream,
        sink,
        bus,
    }
}

async fn paused_mid_synthesis(q: &mut Harness, gate: &Gate, text: &str) -> RequestId {
    let req = speak_req("viewer", text);
    let id = req.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    gate.entered.notified().await;
    q.handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut q.stream, |e| matches!(e, SpeakEvent::Paused { .. })).await;
    gate.release.notify_one();
    id
}

#[tokio::test]
async fn a_clip_synthesized_while_paused_is_held_until_resume() {
    let gate = Arc::new(Gate::default());
    let mut q = spawn_queue(config(), Some(Arc::clone(&gate)));
    let id = paused_mid_synthesis(&mut q, &gate, "sponsor read in progress").await;

    assert_quiet(
        &mut q.stream,
        |e| is_started_with_a_voice(e) || is_finished(e, &id),
        "a clip synthesized during a pause must not start",
    )
    .await;
    assert_eq!(q.sink.plays(), 0, "the sink ran while the queue was paused");

    q.handle.send(SpeakCommand::Resume).await.unwrap();
    wait_for(&mut q.stream, |e| is_finished(e, &id)).await;
    assert_eq!(q.sink.plays(), 1, "resume must play the held clip once");
}

#[tokio::test]
async fn skipping_a_held_clip_drops_it_and_resume_plays_the_next_message() {
    let gate = Arc::new(Gate::default());
    let mut q = spawn_queue(config(), Some(Arc::clone(&gate)));
    let held = paused_mid_synthesis(&mut q, &gate, "held then skipped").await;
    assert_quiet(&mut q.stream, is_started_with_a_voice, "held clip started").await;

    q.handle.send(SpeakCommand::Skip).await.unwrap();
    wait_for(
        &mut q.stream,
        |e| matches!(e, SpeakEvent::Skipped { request_id, .. } if *request_id == held),
    )
    .await;
    let next = speak_req("viewer", "the next message");
    let next_id = next.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(next)).await.unwrap();
    q.handle.send(SpeakCommand::Resume).await.unwrap();
    gate.entered.notified().await;
    gate.release.notify_one();
    wait_for(&mut q.stream, |e| is_finished(e, &next_id)).await;

    assert_eq!(
        q.bus
            .published("speak.skipped", &held)
            .map(|p| p["reason"].clone()),
        Some(serde_json::json!("user_skip"))
    );
    assert_eq!(
        q.sink.plays(),
        1,
        "only the next message may reach the sink, never the skipped held clip"
    );
}

#[tokio::test]
async fn a_synthesis_past_timeout_per_item_fails_as_engine_timeout_and_the_queue_moves_on() {
    let mut q = spawn_queue(
        QueueConfig {
            timeout_per_item: SHORT_SYNTHESIS_TIMEOUT,
            ..config()
        },
        None,
    );
    let stuck = speak_req("viewer", HANGING_TEXT);
    let stuck_id = stuck.request_id.clone();
    let next = speak_req("viewer", "spoken after the stuck one");
    let next_id = next.request_id.clone();

    q.handle.send(SpeakCommand::Enqueue(stuck)).await.unwrap();
    q.handle.send(SpeakCommand::Enqueue(next)).await.unwrap();
    wait_for(&mut q.stream, |e| is_finished(e, &next_id)).await;

    assert_eq!(
        q.bus
            .published("speak.failed", &stuck_id)
            .map(|p| p["reason"].clone()),
        Some(serde_json::json!("engine_timeout"))
    );
    assert_eq!(
        q.sink.plays(),
        1,
        "only the message after the stuck one played"
    );
}

async fn speak_once(q: &mut Harness, viewer: &str) {
    let req = speak_req(viewer, "the message a replay repeats");
    let id = req.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    wait_for(&mut q.stream, |e| is_finished(e, &id)).await;
    q.handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut q.stream, |e| matches!(e, SpeakEvent::Paused { .. })).await;
}

async fn enqueue_admitted(q: &mut Harness, viewer: &str) {
    let req = speak_req(viewer, "already waiting");
    let id = req.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    wait_for(
        &mut q.stream,
        |e| matches!(e, SpeakEvent::Enqueued { request_id, .. } if *request_id == id),
    )
    .await;
}

async fn next_admission(q: &mut Harness) -> SpeakEvent {
    wait_for(&mut q.stream, |e| {
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
        matches!(&admission, SpeakEvent::Rejected { reason, .. } if reason.starts_with("queue full")),
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
        matches!(&admission, SpeakEvent::Rejected { reason, .. } if reason.starts_with("per-user limit")),
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
        .send(SpeakCommand::Enqueue(speak_req(
            "replayed-viewer",
            "one too many",
        )))
        .await
        .unwrap();

    let admission = next_admission(&mut q).await;
    assert!(
        matches!(&admission, SpeakEvent::Rejected { reason, .. } if reason.starts_with("per-user limit")),
        "the replay must have used one of the viewer's slots, got {admission:?}"
    );
}
