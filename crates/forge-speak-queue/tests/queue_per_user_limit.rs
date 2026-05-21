//! Regression: per-user limit must reject requests beyond the configured cap.
//!
//! Invariant: with per_user_limit=N a single viewer may have at most N items
//! pending simultaneously. The (N+1)-th request must be rejected with a
//! `SpeakEvent::Rejected` event — not silently dropped and not panicked.

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
        Ok(PcmBuffer::new(vec![0i16; 100], 22_050, 1))
    }
}

struct FakeFactory;
impl TtsEngineFactory for FakeFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(FakeEngine::new()))
    }
}

struct NullSink;
#[async_trait]
impl AudioSink for NullSink {
    async fn play(&self, _buf: PcmBuffer) -> Result<(), AudioError> {
        Ok(())
    }
}

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

fn make_deps() -> QueueDeps {
    let mut registry = TtsRegistry::new();
    registry.register(EngineId("fake".into()), Arc::new(FakeFactory));
    let resolver = VoiceAliasResolver::new(
        vec![],
        AssignmentStrategy::DeterministicByName,
        IgnoreProfile::default(),
        SynthesisDefaults::default(),
    );
    QueueDeps {
        registry: Arc::new(registry),
        resolver: Arc::new(std::sync::RwLock::new(resolver)),
        pipeline: Arc::new(forge_tts_pipeline::PipelineConfig::default()),
        audio_sink: Arc::new(NullSink),
        event_bus: Arc::new(NullPublisher),
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
        source_event_id: EventId::new(),
    }
}

async fn collect_events(
    stream: &mut forge_speak_queue::SpeakEventStream,
    n: usize,
    max_ms: u64,
) -> Vec<SpeakEvent> {
    let mut events = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(max_ms);
    while events.len() < n {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, stream.recv()).await {
            Ok(Ok(ev)) => events.push(ev),
            Ok(Err(_)) | Err(_) => break,
        }
    }
    events
}

#[tokio::test]
async fn sixth_request_rejected_when_limit_is_five() {
    let config = QueueConfig {
        per_user_limit: 5,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, make_deps());

    // Pause the queue so items accumulate without being popped.
    handle.send(SpeakCommand::Pause).await.unwrap();
    // Drain the Paused event.
    collect_events(&mut stream, 1, 500).await;

    let viewer = "heavy-user";
    for i in 0..5 {
        handle
            .send(SpeakCommand::Enqueue(speak_req(
                viewer,
                &format!("msg {i}"),
            )))
            .await
            .unwrap();
    }

    // 6th request must be rejected.
    handle
        .send(SpeakCommand::Enqueue(speak_req(viewer, "over the limit")))
        .await
        .unwrap();

    // Collect enough events to see the Rejected one.
    let events = collect_events(&mut stream, 15, 1_000).await;
    let rejected_count = events
        .iter()
        .filter(|e| matches!(e, SpeakEvent::Rejected { .. }))
        .count();

    assert_eq!(rejected_count, 1, "exactly one Rejected event expected");
}

#[tokio::test]
async fn different_viewers_are_tracked_independently() {
    // Each viewer has their own counter — viewer B's count must not
    // interfere with viewer A's limit.
    let config = QueueConfig {
        per_user_limit: 2,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, make_deps());
    handle.send(SpeakCommand::Pause).await.unwrap();
    collect_events(&mut stream, 1, 500).await;

    // Enqueue 2 for each viewer — no rejections expected.
    for viewer in ["viewer-a", "viewer-b"] {
        for i in 0..2 {
            handle
                .send(SpeakCommand::Enqueue(speak_req(
                    viewer,
                    &format!("msg {i}"),
                )))
                .await
                .unwrap();
        }
    }

    let events = collect_events(&mut stream, 10, 500).await;
    let rejected_count = events
        .iter()
        .filter(|e| matches!(e, SpeakEvent::Rejected { .. }))
        .count();

    assert_eq!(
        rejected_count, 0,
        "no rejections expected when each viewer is within their own limit"
    );
}
