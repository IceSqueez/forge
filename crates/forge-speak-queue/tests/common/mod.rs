#![allow(dead_code, clippy::unwrap_used, clippy::panic)]

use forge_events::test_support::NullPublisher;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink};
use forge_events::{Event, EventPublisher};
use forge_speak_queue::VoiceAliasResolver;
use forge_speak_queue::{
    Priority, QueueDeps, RequestId, SpeakEvent, SpeakEventStream, SpeakRequest,
};
use forge_speech_types::PcmBuffer;
use forge_speech_types::{
    AliasId, AliasState, AssignmentStrategy, IgnoreProfile, SynthesisDefaults, VoiceAlias,
};
use forge_speech_types::{EngineId, TtsVoice, VoiceGender, VoiceId};
use forge_tts_core::{
    EngineCapabilities, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError, TtsRegistry,
};
use forge_types::EventId;

pub const HANG_GUARD: Duration = Duration::from_secs(30);
const PAUSED_CLOCK_SETTLE: Duration = Duration::from_millis(1);

pub static PLAIN_CAPABILITIES: EngineCapabilities = EngineCapabilities {
    ssml: false,
    neural_voices: false,
    streaming: false,
    custom_lexicons: false,
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
        &PLAIN_CAPABILITIES
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
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Local
    }

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

#[derive(Default)]
pub struct RecordingPublisher {
    events: Mutex<Vec<Event>>,
}

impl EventPublisher for RecordingPublisher {
    fn publish(&self, event: Event) {
        self.events.lock().unwrap().push(event);
    }
}

impl RecordingPublisher {
    pub fn of_kind(&self, kind: &str) -> Vec<Event> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.kind == kind)
            .cloned()
            .collect()
    }

    pub fn payload_of(&self, kind: &str, request_id: &RequestId) -> Option<serde_json::Value> {
        self.of_kind(kind)
            .into_iter()
            .find(|event| event.payload["request_id"] == request_id.0.as_str())
            .map(|event| event.payload)
    }
}

pub fn recording_bus(deps: QueueDeps) -> (QueueDeps, Arc<RecordingPublisher>) {
    let bus = Arc::new(RecordingPublisher::default());
    let event_bus: Arc<dyn EventPublisher> = bus.clone();
    (QueueDeps { event_bus, ..deps }, bus)
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
        pipeline: forge_speak_queue::PipelineConfigHandle::new(neutral_pipeline()),
        audio_sink: sink,
        event_bus: Arc::new(NullPublisher),
        disabled_engines: std::collections::HashSet::new(),
        engine_gains: std::collections::HashMap::new(),
    }
}

pub fn standard_deps(sink: Arc<dyn AudioSink>) -> QueueDeps {
    make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    )
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
        engine_override: None,
        voice_override: None,
        source_event_id: Some(EventId::new()),
        is_reward: false,
        target: None,
        message_emotes: Default::default(),
    }
}

pub fn request_with_priority(viewer: &str, text: &str, priority: Priority) -> SpeakRequest {
    SpeakRequest {
        priority,
        ..request(viewer, text)
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

pub async fn next_event(stream: &mut SpeakEventStream) -> SpeakEvent {
    match tokio::time::timeout(HANG_GUARD, stream.recv()).await {
        Ok(Ok(event)) => event,
        Ok(Err(error)) => panic!("speak event stream ended: {error:?}"),
        Err(_) => panic!("no SpeakEvent arrived within the hang guard"),
    }
}

pub async fn next_matching<F>(stream: &mut SpeakEventStream, pred: F) -> SpeakEvent
where
    F: Fn(&SpeakEvent) -> bool,
{
    loop {
        let event = next_event(stream).await;
        if pred(&event) {
            return event;
        }
    }
}

pub async fn wait_for<F>(stream: &mut SpeakEventStream, pred: F)
where
    F: Fn(&SpeakEvent) -> bool,
{
    next_matching(stream, pred).await;
}

pub fn is_outcome_of(event: &SpeakEvent, id: &RequestId) -> bool {
    match event {
        SpeakEvent::Finished { request_id }
        | SpeakEvent::Failed { request_id, .. }
        | SpeakEvent::Skipped { request_id, .. } => request_id == id,
        _ => false,
    }
}

pub async fn outcome_of(stream: &mut SpeakEventStream, id: &RequestId) -> SpeakEvent {
    next_matching(stream, |event| is_outcome_of(event, id)).await
}

pub async fn enqueue_proving_nothing_started(
    handle: &forge_speak_queue::SpeakQueueHandle,
    stream: &mut SpeakEventStream,
) {
    let proof = request("proof", "proof");
    let proof_id = proof.request_id.clone();
    handle
        .send(forge_speak_queue::SpeakCommand::Enqueue(proof))
        .await
        .unwrap();
    loop {
        match next_event(stream).await {
            SpeakEvent::Started { .. } => panic!("an item started while it was held"),
            SpeakEvent::Enqueued { request_id, .. } if request_id == proof_id => return,
            _ => {}
        }
    }
}

pub async fn first_started_ids(stream: &mut SpeakEventStream, count: usize) -> Vec<RequestId> {
    let mut seen: Vec<RequestId> = Vec::new();
    while seen.len() < count {
        if let SpeakEvent::Started { request_id, .. } = next_event(stream).await
            && !seen.contains(&request_id)
        {
            seen.push(request_id);
        }
    }
    seen
}

pub async fn wait_for_resolved_voice(stream: &mut SpeakEventStream) -> (String, String) {
    loop {
        if let SpeakEvent::Started {
            voice_id: Some(voice_id),
            engine_id: Some(engine_id),
            ..
        } = next_event(stream).await
        {
            return (voice_id.0, engine_id.0);
        }
    }
}

pub async fn settle_on_paused_clock() {
    tokio::time::sleep(PAUSED_CLOCK_SETTLE).await;
}

pub async fn drain_once_settled_on_paused_clock(stream: &mut SpeakEventStream) -> Vec<SpeakEvent> {
    let mut drained = Vec::new();
    while let Ok(received) = tokio::time::timeout(PAUSED_CLOCK_SETTLE, stream.recv()).await {
        drained.push(received.unwrap());
    }
    drained
}

pub async fn assert_no_event_once_settled_on_paused_clock<F>(stream: &mut SpeakEventStream, pred: F)
where
    F: Fn(&SpeakEvent) -> bool,
{
    let drained = drain_once_settled_on_paused_clock(stream).await;
    let unexpected: Vec<&SpeakEvent> = drained.iter().filter(|event| pred(event)).collect();
    assert!(unexpected.is_empty(), "unexpected events: {unexpected:?}");
}

pub fn neutral_output() -> forge_tts_pipeline::OutputConfig {
    forge_tts_pipeline::OutputConfig {
        read_display_name_first: false,
        emote_treatment: forge_tts_pipeline::EmoteTreatment::Keep,
        strip_emoji: false,
        sanitize_punctuation: false,
        max_duration_secs: None,
        language_aware_voice: false,
    }
}

pub fn neutral_pipeline() -> forge_tts_pipeline::PipelineConfig {
    forge_tts_pipeline::PipelineConfig {
        emote_tokens: forge_tts_pipeline::EmoteTokenSet::default(),
        skip_rules: forge_tts_pipeline::SkipRulesConfig::default(),
        replacement_rules: Vec::new(),
        word_blocklist: Vec::new(),
        blocklist_mode: forge_tts_pipeline::BlocklistMode::Censor,
        output: neutral_output(),
        strip_reward_emotes: false,
    }
}
