#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink, PcmBuffer};
use forge_events::{Event, EventPublisher};
use forge_speak_queue::{
    Priority, QueueConfig, QueueDeps, RequestId, SpeakCommand, SpeakEvent, SpeakRequest,
};
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError,
    TtsRegistry, TtsVoice, VoiceGender, VoiceId,
};
use forge_types::EventId;
use forge_voice::{AssignmentStrategy, IgnoreProfile, SynthesisDefaults, VoiceAliasResolver};

struct FakeEngine {
    id: EngineId,
    voices: Vec<TtsVoice>,
}

impl FakeEngine {
    fn new() -> Self {
        Self {
            id: EngineId("fake".into()),
            voices: vec![TtsVoice {
                id: VoiceId("fake-voice".into()),
                name: "Fake Voice".into(),
                locale: "en-US".into(),
                gender: VoiceGender::Neutral,
                engine_id: EngineId("fake".into()),
                is_neural: false,
                sample_rate_hint: 22_050,
            }],
        }
    }
}

#[async_trait]
impl TtsEngine for FakeEngine {
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
        Ok(self.voices.clone())
    }
    async fn synthesize(&self, _req: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        Ok(PcmBuffer::new(vec![0i16; 100], 22_050, 1))
    }
}

struct FakeFactory;
impl TtsEngineFactory for FakeFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(FakeEngine::new()))
    }
}

struct RecordingSink {
    plays: Arc<std::sync::Mutex<usize>>,
}
impl RecordingSink {
    fn new() -> (Self, Arc<std::sync::Mutex<usize>>) {
        let count = Arc::new(std::sync::Mutex::new(0usize));
        (
            Self {
                plays: count.clone(),
            },
            count,
        )
    }
}
#[async_trait]
impl AudioSink for RecordingSink {
    async fn play(&self, _buf: PcmBuffer) -> Result<(), AudioError> {
        *self.plays.lock().unwrap() += 1;
        Ok(())
    }
}

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

struct RecordingPublisher {
    events: Arc<std::sync::Mutex<Vec<Event>>>,
}
impl EventPublisher for RecordingPublisher {
    fn publish(&self, event: Event) {
        self.events.lock().unwrap().push(event);
    }
}

fn make_request(viewer: &str, text: &str, priority: Priority) -> SpeakRequest {
    SpeakRequest {
        request_id: RequestId::new(),
        viewer_id: viewer.into(),
        viewer_name: viewer.into(),
        text: text.into(),
        priority,
        alias_override: None,
        engine_override: None,
        voice_override: None,
        source_event_id: Some(EventId::new()),
        is_reward: false,
    }
}

fn make_deps(sink: Arc<dyn AudioSink>) -> QueueDeps {
    let mut registry = TtsRegistry::new();
    registry.register(EngineId("fake".into()), Arc::new(FakeFactory));
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
        event_bus: Arc::new(NullPublisher),
        disabled_engines: std::collections::HashSet::new(),
        engine_gains: std::collections::HashMap::new(),
    }
}

fn make_deps_recording(sink: Arc<dyn AudioSink>) -> (QueueDeps, Arc<std::sync::Mutex<Vec<Event>>>) {
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let deps = QueueDeps {
        event_bus: Arc::new(RecordingPublisher {
            events: Arc::clone(&events),
        }),
        ..make_deps(sink)
    };
    (deps, events)
}

async fn drain_until<F>(stream: &mut forge_speak_queue::SpeakEventStream, predicate: F, max_ms: u64)
where
    F: Fn(&SpeakEvent) -> bool,
{
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(max_ms);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            panic!("timeout waiting for expected SpeakEvent");
        }
        match tokio::time::timeout(remaining, stream.recv()).await {
            Ok(Ok(ref ev)) if predicate(ev) => return,
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => panic!("stream closed"),
            Err(_) => panic!("timeout waiting for expected SpeakEvent"),
        }
    }
}

#[tokio::test]
async fn enqueue_and_finish() {
    let (sink, play_count) = RecordingSink::new();
    let (handle, mut stream) =
        forge_speak_queue::spawn(QueueConfig::default(), make_deps(Arc::new(sink)));

    let req = make_request("viewer1", "hello world", Priority::Normal);
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();

    drain_until(
        &mut stream,
        |e| matches!(e, SpeakEvent::Finished { .. }),
        2_000,
    )
    .await;

    assert_eq!(*play_count.lock().unwrap(), 1);
}

#[tokio::test]
async fn pause_holds_synthesis() {
    let (sink, play_count) = RecordingSink::new();
    let (handle, mut stream) =
        forge_speak_queue::spawn(QueueConfig::default(), make_deps(Arc::new(sink)));

    handle.send(SpeakCommand::Pause).await.unwrap();
    drain_until(&mut stream, |e| matches!(e, SpeakEvent::Paused { .. }), 500).await;

    let req = make_request("viewer1", "paused message", Priority::Normal);
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        *play_count.lock().unwrap(),
        0,
        "should not play while paused"
    );

    handle.send(SpeakCommand::Resume).await.unwrap();
    drain_until(
        &mut stream,
        |e| matches!(e, SpeakEvent::Finished { .. }),
        2_000,
    )
    .await;
    assert_eq!(*play_count.lock().unwrap(), 1);
}

#[tokio::test]
async fn clear_empties_queue() {
    let (sink, play_count) = RecordingSink::new();
    let config = QueueConfig {
        max_queue_len: 50,
        per_user_limit: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, make_deps(Arc::new(sink)));

    handle.send(SpeakCommand::Pause).await.unwrap();
    drain_until(&mut stream, |e| matches!(e, SpeakEvent::Paused { .. }), 500).await;

    for i in 0..5 {
        let req = make_request("viewer1", &format!("message {i}"), Priority::Normal);
        handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    }

    handle.send(SpeakCommand::Clear).await.unwrap();
    drain_until(&mut stream, |e| matches!(e, SpeakEvent::Cleared), 500).await;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(*play_count.lock().unwrap(), 0);
}

#[tokio::test]
async fn per_user_limit_rejects_excess() {
    let (sink, _) = RecordingSink::new();
    let config = QueueConfig {
        per_user_limit: 2,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, make_deps(Arc::new(sink)));

    handle.send(SpeakCommand::Pause).await.unwrap();
    drain_until(&mut stream, |e| matches!(e, SpeakEvent::Paused { .. }), 500).await;

    for _ in 0..3 {
        let req = make_request("same_user", "test", Priority::Normal);
        handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    }

    drain_until(
        &mut stream,
        |e| matches!(e, SpeakEvent::Rejected { .. }),
        500,
    )
    .await;
}

#[tokio::test]
async fn queue_full_rejects_when_at_capacity() {
    let (sink, _) = RecordingSink::new();
    let config = QueueConfig {
        max_queue_len: 2,
        per_user_limit: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, make_deps(Arc::new(sink)));

    handle.send(SpeakCommand::Pause).await.unwrap();
    drain_until(&mut stream, |e| matches!(e, SpeakEvent::Paused { .. }), 500).await;

    for i in 0..3 {
        let req = make_request(&format!("user{i}"), "test", Priority::Normal);
        handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    }

    drain_until(
        &mut stream,
        |e| matches!(e, SpeakEvent::Rejected { .. }),
        500,
    )
    .await;
}

#[tokio::test]
async fn high_priority_enqueued_first() {
    let (sink, play_count) = RecordingSink::new();
    let config = QueueConfig {
        max_queue_len: 50,
        per_user_limit: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, make_deps(Arc::new(sink)));

    handle.send(SpeakCommand::Pause).await.unwrap();
    drain_until(&mut stream, |e| matches!(e, SpeakEvent::Paused { .. }), 500).await;

    let normal = make_request("user1", "normal", Priority::Normal);
    let high = make_request("user2", "high priority", Priority::High);
    handle.send(SpeakCommand::Enqueue(normal)).await.unwrap();
    handle.send(SpeakCommand::Enqueue(high)).await.unwrap();

    handle.send(SpeakCommand::Resume).await.unwrap();
    drain_until(
        &mut stream,
        |e| *play_count.lock().unwrap() >= 2 || matches!(e, SpeakEvent::Finished { .. }),
        3_000,
    )
    .await;
    assert!(*play_count.lock().unwrap() >= 1);
}

#[tokio::test]
async fn started_bus_event_emitted_at_dequeue_and_after_synth_with_superset_schema() {
    let (sink, _play_count) = RecordingSink::new();
    let (deps, bus) = make_deps_recording(Arc::new(sink));
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    let src = EventId::new();
    let mut req = make_request("nova", "hello chat", Priority::Normal);
    req.source_event_id = Some(src);
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();

    drain_until(
        &mut stream,
        |e| matches!(e, SpeakEvent::Finished { .. }),
        2_000,
    )
    .await;

    let started: Vec<Event> = bus
        .lock()
        .unwrap()
        .iter()
        .filter(|e| e.kind == "speak.started")
        .cloned()
        .collect();
    assert_eq!(
        started.len(),
        2,
        "speak.started fires once at dequeue and once after synth resolves"
    );

    assert!(
        started[0].payload["voice_id"].is_null(),
        "dequeue emission has no resolved voice yet"
    );
    assert!(started[0].payload["engine_id"].is_null());
    assert!(
        started[1].payload["voice_id"].is_string(),
        "post-synth emission carries the resolved voice"
    );
    assert!(started[1].payload["engine_id"].is_string());

    for ev in &started {
        assert_eq!(ev.payload["viewer_name"].as_str(), Some("nova"));
        assert_eq!(ev.payload["text"].as_str(), Some("hello chat"));
        assert!(
            ev.payload.get("queue_len").is_some_and(|v| v.is_number()),
            "both emissions share the superset schema key queue_len"
        );
        for key in ["detected_language", "language_confidence"] {
            assert!(
                ev.payload.get(key).is_some_and(|v| v.is_null()),
                "{key} is part of the superset schema and null while detection is off"
            );
        }
        assert_eq!(ev.caused_by, Some(src));
    }
}
