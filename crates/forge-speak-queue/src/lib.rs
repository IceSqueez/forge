mod actor;
pub mod filters;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use filters::{
    FilterMappingError, PipelineConfigHandle, build_config_lenient, build_config_strict,
};
pub use forge_tts_core::TtsError;
use forge_tts_core::{EngineId, TtsRegistry, TtsVoice, VoiceId};
use forge_types::Shared;
use forge_voice::{AliasId, AssignmentStrategy, SynthesisDefaults, VoiceAlias, VoiceAliasResolver};

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
    /// Head-of-normal-queue but behind other High entries.
    High,
}

#[derive(Debug, Clone)]
pub struct SpeakRequest {
    pub request_id: RequestId,
    pub viewer_id: String,
    pub viewer_name: String,
    pub text: String,
    pub priority: Priority,
    pub alias_override: Option<AliasId>,
    /// Ignored when `voice_override` is set.
    pub engine_override: Option<EngineId>,
    /// Bypasses alias and strategy resolution entirely.
    pub voice_override: Option<VoiceId>,
    /// None when not triggered by an event (UI-invoked preview/test speech).
    pub source_event_id: Option<forge_types::EventId>,
    /// Gates `PipelineConfig::strip_reward_emotes`, independent of `strip_twitch_emotes`.
    pub is_reward: bool,
}

#[derive(Debug)]
pub enum SpeakCommand {
    Enqueue(SpeakRequest),
    Skip,
    PlayNow(RequestId),
    RemoveQueued(RequestId),
    /// No-op if `request_id` or `before` (when set) is absent from the pending queues, or if
    /// `before` equals `request_id`. Moves `request_id` to the tail of `normal_queue` when
    /// `before` is `None`.
    Reorder {
        request_id: RequestId,
        before: Option<RequestId>,
    },
    Clear,
    /// Unlike `Clear`, leaves the in-flight item playing to completion.
    ClearPending,
    Pause,
    Resume,
    Replay,
    /// Upserts by `viewer_id`.
    SetAlias(VoiceAlias),
    /// No-op if the viewer has no alias yet.
    SwitchAlias {
        viewer_id: String,
        engine_id: EngineId,
        voice_id: VoiceId,
    },
    SetStrategy(AssignmentStrategy),
    /// Clamped to `0.0..=1.0`.
    SetVolume(f32),
    SetEngineParams(EngineId, SynthesisDefaults, f32),
    /// No-op if the id is absent.
    RemoveAlias(AliasId),
    /// Send after registering a new engine factory into the live `TtsRegistry` so
    /// the catalog picks it up without an app restart.
    RefreshVoiceCatalog,
    SetEngineEnabled(EngineId, bool),
    VoiceGateActivated,
    VoiceGateDeactivated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedOrderEntry {
    pub request_id: RequestId,
    /// Reflects current queue membership, not the request's original `Priority` - a
    /// reorder can move an item between `high_queue` and `normal_queue` without
    /// touching the field it was enqueued with.
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
        voice_id: VoiceId,
        engine_id: EngineId,
        viewer_name: String,
        text: String,
        /// Zero until synthesis resolves an actual voice, same as `voice_id`/`engine_id`.
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
        error: String,
    },
    Skipped {
        request_id: RequestId,
        reason: String,
    },
    Removed {
        request_id: RequestId,
    },
    Rejected {
        request_id: RequestId,
        reason: String,
    },
    QueueChanged {
        queue_len: usize,
        /// All of `high_queue` in order, then all of `normal_queue` in order - the exact
        /// future playback order.
        order: Vec<QueuedOrderEntry>,
    },
    Paused {
        reason: String,
    },
    Resumed,
    VoiceGateHeld,
    VoiceGateReleased,
    Cleared,
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
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_queue_len: 100,
            per_user_limit: 5,
            master_volume: 1.0,
            timeout_per_item: Duration::from_secs(30),
        }
    }
}

pub struct QueueDeps {
    // std::sync::RwLock here, not tokio - guard never crosses await.
    pub registry: Arc<std::sync::RwLock<TtsRegistry>>,
    pub resolver: Arc<std::sync::RwLock<VoiceAliasResolver>>,
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
    resolver: Arc<std::sync::RwLock<VoiceAliasResolver>>,
    master_volume_bits: Arc<AtomicU32>,
    engine_gains: Shared<HashMap<EngineId, f32>>,
}

impl SpeakQueueHandle {
    pub async fn send(&self, cmd: SpeakCommand) -> Result<(), SpeakError> {
        self.tx.send(cmd).await.map_err(|_| SpeakError::ActorGone)
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

    pub fn engine_synthesis_defaults(&self, engine_id: &EngineId) -> SynthesisDefaults {
        self.resolver
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .defaults_for(engine_id)
    }

    pub fn engine_gain(&self, engine_id: &EngineId) -> f32 {
        self.engine_gains
            .load()
            .get(engine_id)
            .copied()
            .unwrap_or(1.0)
    }

    pub fn master_volume(&self) -> f32 {
        f32::from_bits(self.master_volume_bits.load(Ordering::Relaxed))
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

    /// Each call returns an independent receiver starting from the next published event.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SpeakEvent> {
        self.event_tx.subscribe()
    }
}

pub struct SpeakEventStream(tokio::sync::broadcast::Receiver<SpeakEvent>);

impl SpeakEventStream {
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
    let resolver = deps.resolver.clone();
    let master_volume_bits = Arc::new(AtomicU32::new(config.master_volume.to_bits()));
    let engine_gains = Shared::<HashMap<EngineId, f32>>::new(HashMap::new());

    let event_tx_clone = event_tx.clone();
    let depth_clone = depth.clone();
    let voices_clone = voices.clone();
    let disabled_engines_clone = disabled_engines.clone();
    let master_volume_bits_clone = master_volume_bits.clone();
    let engine_gains_clone = engine_gains.clone();
    tokio::spawn(async move {
        actor::run_actor(
            config,
            deps,
            cmd_rx,
            event_tx_clone,
            depth_clone,
            voices_clone,
            disabled_engines_clone,
            master_volume_bits_clone,
            engine_gains_clone,
        )
        .await;
    });

    (
        SpeakQueueHandle {
            tx: cmd_tx,
            event_tx,
            depth,
            voices,
            disabled_engines,
            resolver,
            master_volume_bits,
            engine_gains,
        },
        SpeakEventStream(event_rx),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handle_send_returns_actor_gone_after_drop() {
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<SpeakCommand>(1);
        let (event_tx, _event_rx) = tokio::sync::broadcast::channel::<SpeakEvent>(1);
        drop(cmd_rx);
        let handle = SpeakQueueHandle {
            tx: cmd_tx,
            event_tx,
            depth: Arc::new(AtomicUsize::new(0)),
            voices: Shared::new(Vec::new()),
            disabled_engines: Shared::new(HashSet::new()),
            resolver: Arc::new(std::sync::RwLock::new(VoiceAliasResolver::new(
                vec![],
                AssignmentStrategy::default(),
                forge_voice::IgnoreProfile::default(),
                SynthesisDefaults::default(),
            ))),
            master_volume_bits: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            engine_gains: Shared::new(HashMap::new()),
        };
        let result = handle.send(SpeakCommand::Skip).await;
        assert!(matches!(result, Err(SpeakError::ActorGone)));
    }

    #[tokio::test]
    async fn subscribe_returns_new_receiver() {
        let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::channel::<SpeakCommand>(1);
        let (event_tx, _event_rx) = tokio::sync::broadcast::channel::<SpeakEvent>(8);
        let handle = SpeakQueueHandle {
            tx: cmd_tx,
            event_tx: event_tx.clone(),
            depth: Arc::new(AtomicUsize::new(0)),
            voices: Shared::new(Vec::new()),
            disabled_engines: Shared::new(HashSet::new()),
            resolver: Arc::new(std::sync::RwLock::new(VoiceAliasResolver::new(
                vec![],
                AssignmentStrategy::default(),
                forge_voice::IgnoreProfile::default(),
                SynthesisDefaults::default(),
            ))),
            master_volume_bits: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            engine_gains: Shared::new(HashMap::new()),
        };
        let mut sub = handle.subscribe();
        event_tx
            .send(SpeakEvent::Cleared)
            .expect("send must succeed");
        let received = sub.try_recv().expect("receiver must see the event");
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
