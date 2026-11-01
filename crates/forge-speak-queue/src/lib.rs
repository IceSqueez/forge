mod actor;
mod catalog;
mod detector_builds;
mod emitter;
mod filters;
mod queues;
pub mod vocabulary;
mod voice;

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use forge_net::{TaskOwner, TaskShutdown};
use serde::{Deserialize, Serialize};

pub use filters::{
    FilterMappingError, PipelineConfigHandle, ReplacementSource, build_config,
    configured_skip_prefix, effective_skip_prefix, emote_treatment, repeat_window,
    store_emote_treatment,
};
pub use forge_audio::PlaybackTarget;
use forge_audio::TargetedSinkFactory;
use forge_speech_types::{AliasId, AssignmentStrategy, SynthesisDefaults, VoiceAlias};
use forge_speech_types::{EngineId, TtsVoice, VoiceId};
pub use forge_tts_core::TtsError;
use forge_tts_core::{RegisteredEngine, TtsEngineFactory, TtsRegistry};
pub use forge_tts_pipeline::{EmoteTokenSet, EmoteTreatment};
use forge_types::{MessageRef, Shared};
pub use voice::{AliasKey, ResolveResult, VoiceAliasResolver, VoiceSkip};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub String);

impl RequestId {
    pub fn new() -> Self {
        Self(ulid::Ulid::generate().to_string())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Normal,
    High,
}

#[derive(Debug, Clone)]
pub struct SpeakRequest {
    pub request_id: RequestId,
    pub viewer_id: String,
    pub viewer_name: String,
    pub text: String,
    pub priority: Priority,
    pub engine_override: Option<EngineId>,
    pub voice_override: Option<VoiceId>,
    pub source_event_id: Option<forge_types::EventId>,
    pub is_reward: bool,
    pub target: Option<PlaybackTarget>,
    pub message_emotes: EmoteTokenSet,
}

#[derive(Debug)]
pub enum SpeakCommand {
    Enqueue(SpeakRequest),
    Shutdown,
    Skip,
    PlayNow(RequestId),
    RemoveQueued(RequestId),
    Cancel(RequestId),
    Reorder {
        request_id: RequestId,
        before: Option<RequestId>,
    },
    Clear,
    ClearPending,
    Pause,
    Resume,
    Replay,
    SetAlias(VoiceAlias),
    SwitchAlias {
        viewer_id: String,
        engine_id: EngineId,
        voice_id: VoiceId,
    },
    SetStrategy(AssignmentStrategy),
    SetVolume(f32),
    SetRepeatWindow(NonZeroUsize),
    SetEngineParams(EngineId, SynthesisDefaults, f32),
    RemoveAlias(AliasId),
    RefreshVoiceCatalog,
    RegisterEngine(EngineRegistration),
    SetEngineEnabled(EngineId, bool),
    VoiceGateActivated,
    VoiceGateDeactivated,
}

pub struct EngineRegistration(pub Arc<dyn TtsEngineFactory>);

impl std::fmt::Debug for EngineRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineRegistration")
            .field("id", &self.0.engine_id())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedOrderEntry {
    pub request_id: RequestId,
    pub is_high_priority: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpeakEvent {
    Enqueued {
        request_id: RequestId,
        queue_len: usize,
        viewer_name: String,
        text: String,
        is_high_priority: bool,
        voice_preview: String,
        estimated_secs: u32,
    },
    Started {
        request_id: RequestId,
        voice_id: Option<VoiceId>,
        engine_id: Option<EngineId>,
        viewer_name: String,
        text: String,
        duration_secs: u32,
    },
    Progress {
        request_id: RequestId,
        elapsed_secs: u32,
    },
    Finished {
        request_id: RequestId,
    },
    Failed {
        request_id: RequestId,
        message: MessageRef,
    },
    Skipped {
        request_id: RequestId,
        message: MessageRef,
    },
    Removed {
        request_id: RequestId,
    },
    Rejected {
        request_id: RequestId,
        reason: vocabulary::RejectReason,
        message: MessageRef,
    },
    QueueChanged {
        queue_len: usize,
        order: Vec<QueuedOrderEntry>,
    },
    Paused,
    Resumed,
    VoiceGateHeld,
    VoiceGateReleased,
    Cleared,
    VoiceCatalogChanged,
}

#[derive(Debug, thiserror::Error)]
pub enum SpeakError {
    #[error("speak queue actor has stopped")]
    ActorGone,

    #[error("subscriber is lagging; events were dropped")]
    LaggingReceiver,
}

pub struct QueueConfig {
    pub max_queue_len: usize,
    pub per_user_limit: usize,
    pub master_volume: f32,
    pub timeout_per_item: Duration,
    pub repeat_window: NonZeroUsize,
}

const DEFAULT_MAX_QUEUE_LEN: usize = 100;
const DEFAULT_PER_USER_LIMIT: usize = 5;
pub(crate) const UNITY_GAIN: f32 = 1.0;
const MUTE_GAIN: f32 = 0.0;
const DEFAULT_MASTER_VOLUME: f32 = UNITY_GAIN;

pub(crate) fn clamp_gain(gain: f32) -> f32 {
    gain.clamp(MUTE_GAIN, UNITY_GAIN)
}
const QUEUE_TIMEOUT_MARGIN: Duration = Duration::from_secs(5);
const DEFAULT_SYNTHESIS_TIMEOUT: Duration =
    forge_tts_core::ENGINE_SYNTHESIS_BUDGET.saturating_add(QUEUE_TIMEOUT_MARGIN);

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_queue_len: DEFAULT_MAX_QUEUE_LEN,
            per_user_limit: DEFAULT_PER_USER_LIMIT,
            master_volume: DEFAULT_MASTER_VOLUME,
            timeout_per_item: DEFAULT_SYNTHESIS_TIMEOUT,
            repeat_window: repeat_window(&forge_storage::TtsPipelineSettings::default()),
        }
    }
}

pub struct QueueDeps {
    pub registry: TtsRegistry,
    pub resolver: VoiceAliasResolver,
    pub pipeline: PipelineConfigHandle,
    pub audio_sink: Arc<dyn forge_audio::AudioSink>,
    pub event_bus: Arc<dyn forge_events::EventPublisher>,
    pub disabled_engines: HashSet<EngineId>,
    pub engine_gains: HashMap<EngineId, f32>,
}

#[derive(Clone)]
pub struct SpeakQueueHandle {
    tx: tokio::sync::mpsc::Sender<SpeakCommand>,
    event_tx: tokio::sync::broadcast::Sender<SpeakEvent>,
    depth: Arc<AtomicUsize>,
    voices: Shared<Vec<TtsVoice>>,
    disabled_engines: Shared<HashSet<EngineId>>,
    master_volume_bits: Arc<AtomicU32>,
    engine_gains: Shared<HashMap<EngineId, f32>>,
    resolver: Shared<VoiceAliasResolver>,
    registry: Shared<TtsRegistry>,
    targeted_legs: TargetedLegs,
    language_detector_ready: tokio::sync::watch::Receiver<bool>,
    tasks: TaskOwner,
}

pub(crate) type TargetedLegs = Shared<Option<Arc<dyn TargetedSinkFactory>>>;

impl SpeakQueueHandle {
    pub async fn shutdown(&self, grace: Duration) -> TaskShutdown {
        if let Err(error) = self.tx.try_send(SpeakCommand::Shutdown) {
            tracing::debug!(%error, "speak queue could not take the stop command");
        }
        self.tasks.shutdown(grace).await
    }

    pub async fn language_detector_ready(&self) -> Result<(), SpeakError> {
        self.language_detector_ready
            .clone()
            .wait_for(|ready| *ready)
            .await
            .map(|_| ())
            .map_err(|_| SpeakError::ActorGone)
    }

    pub async fn send(&self, cmd: SpeakCommand) -> Result<(), SpeakError> {
        self.tx.send(cmd).await.map_err(|_| SpeakError::ActorGone)
    }

    pub fn install_targeted_legs(&self, factory: Arc<dyn TargetedSinkFactory>) {
        self.targeted_legs.store(Some(factory));
    }

    pub fn queue_depth(&self) -> usize {
        self.depth.load(Ordering::Relaxed)
    }

    pub fn available_voices(&self) -> Arc<Vec<TtsVoice>> {
        self.voices.load()
    }

    pub fn disabled_engines(&self) -> Arc<HashSet<EngineId>> {
        self.disabled_engines.load()
    }

    pub fn engine_gain(&self, engine_id: &EngineId) -> f32 {
        self.engine_gains
            .load()
            .get(engine_id)
            .copied()
            .unwrap_or(UNITY_GAIN)
    }

    pub fn master_volume(&self) -> f32 {
        f32::from_bits(self.master_volume_bits.load(Ordering::Relaxed))
    }

    pub fn resolver_snapshot(&self) -> Arc<VoiceAliasResolver> {
        self.resolver.load()
    }

    pub fn engine_roster(&self) -> Vec<RegisteredEngine> {
        self.registry.load().roster()
    }

    pub fn registered_engine_ids(&self) -> Vec<EngineId> {
        self.registry.load().engine_ids()
    }

    pub fn engines(&self) -> Vec<EngineId> {
        let voices = self.available_voices();
        let mut engines: Vec<EngineId> = Vec::new();
        for voice in voices.iter() {
            if !engines.contains(&voice.engine_id) {
                engines.push(voice.engine_id.clone());
            }
        }
        engines
    }

    pub async fn notify_voicegate_active(&self) -> Result<(), SpeakError> {
        self.send(SpeakCommand::VoiceGateActivated).await
    }

    pub async fn notify_voicegate_inactive(&self) -> Result<(), SpeakError> {
        self.send(SpeakCommand::VoiceGateDeactivated).await
    }

    pub fn subscribe(&self) -> SpeakEventStream {
        SpeakEventStream(self.event_tx.subscribe())
    }
}

pub struct SpeakEventStream(tokio::sync::broadcast::Receiver<SpeakEvent>);

impl SpeakEventStream {
    #[cfg(feature = "test-support")]
    pub fn from_broadcast(receiver: tokio::sync::broadcast::Receiver<SpeakEvent>) -> Self {
        Self(receiver)
    }

    pub async fn recv(&mut self) -> Result<SpeakEvent, SpeakError> {
        match self.0.recv().await {
            Ok(event) => Ok(event),
            Err(tokio::sync::broadcast::error::RecvError::Closed) => Err(SpeakError::ActorGone),
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                Err(SpeakError::LaggingReceiver)
            }
        }
    }
}

pub fn spawn(config: QueueConfig, deps: QueueDeps) -> (SpeakQueueHandle, SpeakEventStream) {
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<SpeakCommand>(256);
    let (event_tx, event_rx) = tokio::sync::broadcast::channel::<SpeakEvent>(256);

    let depth = Arc::new(AtomicUsize::new(0));
    let voices = Shared::<Vec<TtsVoice>>::new(Vec::new());
    let disabled_engines = Shared::<HashSet<EngineId>>::new(HashSet::new());
    let master_volume_bits = Arc::new(AtomicU32::new(config.master_volume.to_bits()));
    let engine_gains = Shared::<HashMap<EngineId, f32>>::new(HashMap::new());
    let targeted_legs = TargetedLegs::new(None);
    let (detector_ready_tx, language_detector_ready) = tokio::sync::watch::channel(false);
    let resolver = Shared::new(deps.resolver);
    let registry = Shared::new(deps.registry);
    let tasks = TaskOwner::new();
    let actor_deps = actor::ActorDeps {
        registry: registry.clone(),
        resolver: resolver.clone(),
        pipeline: deps.pipeline,
        audio_sink: deps.audio_sink,
        event_bus: deps.event_bus,
        disabled_engines: deps.disabled_engines,
        engine_gains: deps.engine_gains,
        tasks: tasks.clone(),
    };

    let actor_handles = actor::ActorHandles {
        depth: depth.clone(),
        voices: voices.clone(),
        disabled_engines: disabled_engines.clone(),
        master_volume_bits: master_volume_bits.clone(),
        engine_gains: engine_gains.clone(),
        targeted_legs: targeted_legs.clone(),
    };
    let actor_event_tx = event_tx.clone();
    tasks.spawn(actor::run_actor(
        config,
        actor_deps,
        actor_handles,
        cmd_rx,
        actor_event_tx,
        detector_ready_tx,
    ));

    (
        SpeakQueueHandle {
            tx: cmd_tx,
            event_tx,
            depth,
            voices,
            disabled_engines,
            master_volume_bits,
            engine_gains,
            resolver,
            registry,
            targeted_legs,
            language_detector_ready,
            tasks,
        },
        SpeakEventStream(event_rx),
    )
}

pub use filters::BuiltConfig;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_handle(
        cmd_tx: tokio::sync::mpsc::Sender<SpeakCommand>,
        event_tx: tokio::sync::broadcast::Sender<SpeakEvent>,
    ) -> SpeakQueueHandle {
        SpeakQueueHandle {
            tx: cmd_tx,
            event_tx,
            depth: Arc::new(AtomicUsize::new(0)),
            voices: Shared::new(Vec::new()),
            disabled_engines: Shared::new(HashSet::new()),
            master_volume_bits: Arc::new(AtomicU32::new(UNITY_GAIN.to_bits())),
            engine_gains: Shared::new(HashMap::new()),
            resolver: Shared::new(VoiceAliasResolver::new(
                Vec::new(),
                AssignmentStrategy::DeterministicByName,
                forge_speech_types::IgnoreProfile::default(),
                SynthesisDefaults::default(),
            )),
            registry: Shared::new(TtsRegistry::new()),
            targeted_legs: Shared::new(None),
            language_detector_ready: tokio::sync::watch::channel(false).1,
            tasks: TaskOwner::new(),
        }
    }

    #[tokio::test]
    async fn handle_send_returns_actor_gone_after_drop() {
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<SpeakCommand>(1);
        let (event_tx, _event_rx) = tokio::sync::broadcast::channel::<SpeakEvent>(1);
        drop(cmd_rx);
        let handle = test_handle(cmd_tx, event_tx);
        let result = handle.send(SpeakCommand::Skip).await;
        assert!(matches!(result, Err(SpeakError::ActorGone)));
    }

    #[tokio::test]
    async fn a_subscriber_receives_events_sent_after_subscribing() {
        let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::channel::<SpeakCommand>(1);
        let (event_tx, _event_rx) = tokio::sync::broadcast::channel::<SpeakEvent>(8);
        let handle = test_handle(cmd_tx, event_tx.clone());
        let mut sub = handle.subscribe();
        event_tx
            .send(SpeakEvent::Cleared)
            .expect("send must succeed");
        let received = sub.recv().await.expect("receiver must see the event");
        assert!(matches!(received, SpeakEvent::Cleared));
    }

    #[tokio::test]
    async fn lagging_receiver_surfaces_lagging_error_and_stream_stays_usable() {
        let (tx, rx) = tokio::sync::broadcast::channel::<SpeakEvent>(2);
        let mut stream = SpeakEventStream(rx);
        for queue_len in 0..4 {
            tx.send(SpeakEvent::QueueChanged {
                queue_len,
                order: vec![],
            })
            .expect("send must succeed while a receiver is alive");
        }

        let lagged = stream.recv().await;
        assert!(matches!(lagged, Err(SpeakError::LaggingReceiver)));

        let next = stream.recv().await;
        assert!(matches!(next, Ok(SpeakEvent::QueueChanged { .. })));
    }

    #[tokio::test]
    async fn closed_sender_surfaces_actor_gone() {
        let (tx, rx) = tokio::sync::broadcast::channel::<SpeakEvent>(2);
        let mut stream = SpeakEventStream(rx);
        drop(tx);

        let result = stream.recv().await;
        assert!(matches!(result, Err(SpeakError::ActorGone)));
    }
}
