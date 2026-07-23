#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink, PcmBuffer};
use forge_events::{Event, EventPublisher};
use forge_speak_queue::{
    Priority, QueueDeps, RequestId, SpeakEvent, SpeakEventStream, SpeakRequest,
};
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError,
    TtsRegistry, TtsVoice, VoiceGender, VoiceId,
};
use forge_types::EventId;
use forge_voice::{
    AliasId, AliasState, AssignmentStrategy, IgnoreProfile, SynthesisDefaults, VoiceAlias,
    VoiceAliasResolver,
};

pub fn voice(id: &str, engine: &str) -> TtsVoice {
    TtsVoice {
        id: VoiceId(id.into()),
        name: id.into(),
        locale: "en-US".into(),
        gender: VoiceGender::Neutral,
        engine_id: EngineId(engine.into()),
        is_neural: false,
        sample_rate_hint: 22_050,
    }
}

struct FakeEngine {
    id: EngineId,
    voices: Vec<TtsVoice>,
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
        Ok(PcmBuffer::new(vec![0i16; 8], 22_050, 1))
    }
}

pub struct FakeFactory {
    pub id: String,
    pub voices: Vec<TtsVoice>,
}

impl TtsEngineFactory for FakeFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(FakeEngine {
            id: EngineId(self.id.clone()),
            voices: self.voices.clone(),
        }))
    }
}

pub fn standard_registry() -> TtsRegistry {
    let mut reg = TtsRegistry::new();
    reg.register(
        EngineId("alpha".into()),
        Arc::new(FakeFactory {
            id: "alpha".into(),
            voices: vec![voice("alpha-1", "alpha"), voice("alpha-2", "alpha")],
        }),
    );
    reg.register(
        EngineId("beta".into()),
        Arc::new(FakeFactory {
            id: "beta".into(),
            voices: vec![voice("beta-1", "beta")],
        }),
    );
    reg
}

pub struct RecordingSink {
    plays: Arc<std::sync::Mutex<usize>>,
}

#[async_trait]
impl AudioSink for RecordingSink {
    async fn play(&self, _buf: PcmBuffer) -> Result<(), AudioError> {
        *self.plays.lock().unwrap() += 1;
        Ok(())
    }
}

pub fn recording_sink() -> (Arc<dyn AudioSink>, Arc<std::sync::Mutex<usize>>) {
    let plays = Arc::new(std::sync::Mutex::new(0usize));
    (
        Arc::new(RecordingSink {
            plays: plays.clone(),
        }),
        plays,
    )
}

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

pub fn make_deps(
    registry: TtsRegistry,
    sink: Arc<dyn AudioSink>,
    strategy: AssignmentStrategy,
    aliases: Vec<VoiceAlias>,
) -> QueueDeps {
    let resolver = VoiceAliasResolver::new(
        aliases,
        strategy,
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

pub fn alias(viewer_id: &str, engine: &str, voice_id: &str) -> VoiceAlias {
    VoiceAlias {
        id: AliasId::new(),
        viewer_id: viewer_id.into(),
        viewer_name: viewer_id.into(),
        engine_id: EngineId(engine.into()),
        voice_id: VoiceId(voice_id.into()),
        pitch_semitones: None,
        rate_multiplier: None,
        state: AliasState::Active,
    }
}

pub fn request(viewer: &str, text: &str) -> SpeakRequest {
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
    }
}

pub fn request_with_overrides(
    viewer: &str,
    engine_override: Option<&str>,
    voice_override: Option<&str>,
) -> SpeakRequest {
    SpeakRequest {
        engine_override: engine_override.map(|e| EngineId(e.into())),
        voice_override: voice_override.map(|v| VoiceId(v.into())),
        ..request(viewer, "override test")
    }
}

pub async fn wait_for<F>(stream: &mut SpeakEventStream, pred: F, max_ms: u64)
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
            Err(_) => panic!("timeout waiting for expected SpeakEvent"),
        }
    }
}

pub async fn wait_for_resolved_voice(
    stream: &mut SpeakEventStream,
    max_ms: u64,
) -> (String, String) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(max_ms);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            panic!("timeout waiting for resolved Started event");
        }
        match tokio::time::timeout(remaining, stream.recv()).await {
            Ok(Ok(SpeakEvent::Started {
                voice_id,
                engine_id,
                ..
            })) if !voice_id.0.is_empty() => return (voice_id.0, engine_id.0),
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => panic!("stream closed"),
            Err(_) => panic!("timeout waiting for resolved Started event"),
        }
    }
}

pub async fn assert_no_event<F>(stream: &mut SpeakEventStream, pred: F, window_ms: u64)
where
    F: Fn(&SpeakEvent) -> bool,
{
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(window_ms);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return;
        }
        match tokio::time::timeout(remaining, stream.recv()).await {
            Ok(Ok(ref ev)) if pred(ev) => panic!("unexpected event arrived within window"),
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => return,
            Err(_) => return,
        }
    }
}
