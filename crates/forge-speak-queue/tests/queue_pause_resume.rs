//! Regression: pause must hold dispatch; resume must continue.
//!
//! Invariant: while paused, no synthesis starts regardless of queue contents.
//! VoiceGate pause uses a separate flag from manual pause; both must be
//! independently releasable (manual Resume only clears the manual flag,
//! VoiceGateDeactivated only clears the gate flag).

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

struct CountingSink {
    plays: Arc<std::sync::Mutex<usize>>,
}

impl CountingSink {
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
impl AudioSink for CountingSink {
    async fn play(&self, _buf: PcmBuffer) -> Result<(), AudioError> {
        *self.plays.lock().unwrap() += 1;
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
    }
}

fn speak_req(text: &str) -> SpeakRequest {
    SpeakRequest {
        request_id: RequestId::new(),
        viewer_id: "viewer".into(),
        viewer_name: "viewer".into(),
        text: text.into(),
        priority: Priority::Normal,
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
async fn pause_prevents_synthesis_until_resume() {
    let (sink, play_count) = CountingSink::new();
    let config = QueueConfig {
        per_user_limit: 10,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, make_deps(Arc::new(sink)));

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused { .. }), 500).await;

    handle
        .send(SpeakCommand::Enqueue(speak_req("paused message")))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        *play_count.lock().unwrap(),
        0,
        "sink must not be called while paused"
    );

    handle.send(SpeakCommand::Resume).await.unwrap();
    wait_for(
        &mut stream,
        |e| matches!(e, SpeakEvent::Finished { .. }),
        2_000,
    )
    .await;

    assert_eq!(
        *play_count.lock().unwrap(),
        1,
        "sink must be called once after resume"
    );
}

#[tokio::test]
async fn voicegate_pause_independent_from_manual_pause() {
    // Manual Resume must not release a VoiceGate-induced pause.
    // VoiceGateDeactivated must release it.
    let (sink, play_count) = CountingSink::new();
    let config = QueueConfig {
        per_user_limit: 10,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, make_deps(Arc::new(sink)));

    // Activate VoiceGate (mic threshold crossed).
    handle.send(SpeakCommand::VoiceGateActivated).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused { .. }), 500).await;

    handle
        .send(SpeakCommand::Enqueue(speak_req("mic is hot")))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert_eq!(
        *play_count.lock().unwrap(),
        0,
        "must not synthesize while VoiceGate active"
    );

    // VoiceGate deactivated — queue should drain.
    handle
        .send(SpeakCommand::VoiceGateDeactivated)
        .await
        .unwrap();
    wait_for(
        &mut stream,
        |e| matches!(e, SpeakEvent::Finished { .. }),
        2_000,
    )
    .await;
    assert_eq!(
        *play_count.lock().unwrap(),
        1,
        "must synthesize after VoiceGate deactivated"
    );
}
