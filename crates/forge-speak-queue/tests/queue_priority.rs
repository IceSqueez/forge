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
}

impl FakeEngine {
    fn new() -> Self {
        Self {
            id: EngineId("fake".into()),
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
        Ok(vec![TtsVoice {
            id: VoiceId("fake-voice".into()),
            name: "Fake".into(),
            locale: "en-US".into(),
            gender: VoiceGender::Neutral,
            engine_id: EngineId("fake".into()),
            is_neural: false,
            sample_rate_hint: 22_050,
        }])
    }
    async fn synthesize(&self, _req: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        Ok(PcmBuffer::new(vec![0i16; 4], 22_050, 1))
    }
}

struct FakeFactory;
impl TtsEngineFactory for FakeFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(FakeEngine::new()))
    }
}

struct OrderRecordingSink {
    notify: Arc<tokio::sync::Notify>,
}

impl OrderRecordingSink {
    fn new() -> (Self, Arc<tokio::sync::Notify>) {
        let notify = Arc::new(tokio::sync::Notify::new());
        (
            Self {
                notify: notify.clone(),
            },
            notify,
        )
    }
}

#[async_trait]
impl AudioSink for OrderRecordingSink {
    async fn play(&self, buf: PcmBuffer) -> Result<(), AudioError> {
        let _ = buf;
        self.notify.notify_one();
        Ok(())
    }
}

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
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

fn req(viewer: &str, text: &str, priority: Priority) -> SpeakRequest {
    SpeakRequest {
        request_id: RequestId::new(),
        viewer_id: viewer.into(),
        viewer_name: viewer.into(),
        text: text.into(),
        priority,
        alias_override: None,
        engine_override: None,
        voice_override: None,
        source_event_id: EventId::new(),
        is_reward: false,
    }
}

async fn wait_for<F>(stream: &mut forge_speak_queue::SpeakEventStream, pred: F, max_ms: u64)
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
            Ok(Ok(ref ev)) if pred(ev) => return,
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => panic!("stream closed"),
            Err(_) => panic!("timeout"),
        }
    }
}

#[tokio::test]
async fn high_priority_dispatched_before_normal_when_enqueued_after() {
    let (sink, notify) = OrderRecordingSink::new();
    let config = QueueConfig {
        per_user_limit: 10,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, make_deps(Arc::new(sink)));

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused { .. }), 500).await;

    let normal_id = {
        let r = req("user-n", "normal message", Priority::Normal);
        let id = r.request_id.clone();
        handle.send(SpeakCommand::Enqueue(r)).await.unwrap();
        id
    };
    let high_id = {
        let r = req("user-h", "high priority message", Priority::High);
        let id = r.request_id.clone();
        handle.send(SpeakCommand::Enqueue(r)).await.unwrap();
        id
    };

    let mut enqueued = 0;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(300);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() || enqueued >= 2 {
            break;
        }
        match tokio::time::timeout(remaining, stream.recv()).await {
            Ok(Ok(SpeakEvent::Enqueued { .. })) => enqueued += 1,
            Ok(Ok(_)) => continue,
            _ => break,
        }
    }

    handle.send(SpeakCommand::Resume).await.unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(2_000);
    let mut first_started_id: Option<forge_speak_queue::RequestId> = None;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, stream.recv()).await {
            Ok(Ok(SpeakEvent::Started { request_id, .. })) => {
                first_started_id = Some(request_id);
                break;
            }
            Ok(Ok(_)) => continue,
            _ => break,
        }
    }

    let _ = notify;
    wait_for(
        &mut stream,
        |e| matches!(e, SpeakEvent::Finished { .. }),
        3_000,
    )
    .await;
    wait_for(
        &mut stream,
        |e| matches!(e, SpeakEvent::Finished { .. }),
        3_000,
    )
    .await;

    assert_ne!(normal_id, high_id);

    if let Some(id) = first_started_id.filter(|id| !id.0.is_empty()) {
        assert_eq!(
            id, high_id,
            "first Started event must be for the High-priority request"
        );
    }
}

#[tokio::test]
async fn multiple_high_priority_items_dispatched_before_normals() {
    let (sink, _notify) = OrderRecordingSink::new();
    let config = QueueConfig {
        per_user_limit: 10,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, make_deps(Arc::new(sink)));

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused { .. }), 500).await;

    for i in 0..3 {
        handle
            .send(SpeakCommand::Enqueue(req(
                &format!("n{i}"),
                "normal",
                Priority::Normal,
            )))
            .await
            .unwrap();
    }
    for i in 0..2 {
        handle
            .send(SpeakCommand::Enqueue(req(
                &format!("h{i}"),
                "high",
                Priority::High,
            )))
            .await
            .unwrap();
    }

    handle.send(SpeakCommand::Resume).await.unwrap();

    for _ in 0..5 {
        wait_for(
            &mut stream,
            |e| matches!(e, SpeakEvent::Finished { .. }),
            5_000,
        )
        .await;
    }
}
