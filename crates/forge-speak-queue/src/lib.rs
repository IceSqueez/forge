mod actor;
pub mod filters;

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use filters::{
    FilterMappingError, PipelineConfigHandle, build_config_lenient, build_config_strict,
};
pub use forge_tts_core::TtsError;
use forge_tts_core::{EngineId, TtsRegistry, TtsVoice, VoiceId};
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
    /// Bits/sub/channel-point rewards - head-of-normal-queue but behind other High.
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
    /// Forces synthesis through this engine, picking a voice from its catalog via
    /// the resolver strategy. Ignored when `voice_override` is set.
    pub engine_override: Option<EngineId>,
    /// Forces this exact voice, bypassing alias and strategy resolution. The engine
    /// is taken from `engine_override` if set, else inferred from the voice catalog.
    pub voice_override: Option<VoiceId>,
    pub source_event_id: forge_types::EventId,
    /// Set when this message originated from a Twitch channel-points reward
    /// redemption. Gates `PipelineConfig::strip_reward_emotes` in the actor,
    /// independently of the `strip_twitch_emotes`-driven stripping applied to
    /// every message.
    pub is_reward: bool,
}

#[derive(Debug)]
pub enum SpeakCommand {
    Enqueue(SpeakRequest),
    Skip,
    Clear,
    /// Drops every pending item but lets the in-flight synthesis/playback finish.
    /// `Clear` also abandons the active item; `ClearPending` deliberately does not.
    ClearPending,
    Pause,
    Resume,
    Replay,
    /// Inserts or replaces (by `viewer_id`) an alias in the live resolver.
    SetAlias(VoiceAlias),
    /// Repoints an existing viewer's alias to a different voice; no-op when the
    /// viewer has no alias yet (use `SetAlias` to create one).
    SwitchAlias {
        viewer_id: String,
        engine_id: EngineId,
        voice_id: VoiceId,
    },
    /// Replaces the fallback strategy applied to viewers without a manual alias.
    SetStrategy(AssignmentStrategy),
    /// Sets `QueueConfig::master_volume`, clamped to `0.0..=1.0`.
    SetVolume(f32),
    SetSynthesisDefaults(SynthesisDefaults),
    /// Drops an alias from the live resolver by id; no-op when the id is absent.
    RemoveAlias(AliasId),
    /// Rebuilds the voice catalog from the live `TtsRegistry`. Send this after
    /// registering a new engine factory into the same registry `Arc` so the
    /// catalog (and `SpeakQueueHandle::engines`/`available_voices`) picks it up
    /// without an app restart.
    RefreshVoiceCatalog,
    /// Disables or re-enables an engine. A disabled engine's voices are excluded
    /// from the rebuilt catalog, which in turn removes it from resolution,
    /// `available_voices()`, and `engines()` without unregistering the factory.
    SetEngineEnabled(EngineId, bool),
    /// Sent by `forge-audio` when the VoiceGate mic threshold is crossed.
    VoiceGateActivated,
    /// Sent by `forge-audio` when the VoiceGate mic level drops below threshold.
    VoiceGateDeactivated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpeakEvent {
    Enqueued {
        request_id: RequestId,
        queue_len: usize,
        viewer_name: String,
        text: String,
        is_high_priority: bool,
    },
    Started {
        request_id: RequestId,
        voice_id: VoiceId,
        engine_id: EngineId,
        viewer_name: String,
        text: String,
        /// Zero until synthesis resolves an actual voice (the pre-synthesis
        /// `Started` ships this as 0, same as `voice_id`/`engine_id`).
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
    Rejected {
        request_id: RequestId,
        reason: String,
    },
    QueueChanged {
        queue_len: usize,
    },
    Paused {
        reason: String,
    },
    Resumed,
    Cleared,
}

#[derive(Debug, thiserror::Error)]
pub enum SpeakError {
    #[error("speak queue is full (max {max})")]
    QueueFull { max: usize },

    #[error("per-user limit reached for viewer {viewer_id}")]
    PerUserLimitReached { viewer_id: String },

    #[error("no voices available for synthesis")]
    NoVoiceAvailable,

    #[error("synthesis failed: {0}")]
    Synthesis(#[from] TtsError),

    #[error("speak queue actor has stopped")]
    ActorGone,
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
    /// Loaded from persisted settings at boot so the first catalog build already
    /// excludes these engines.
    pub disabled_engines: HashSet<EngineId>,
}

#[derive(Clone)]
pub struct SpeakQueueHandle {
    tx: tokio::sync::mpsc::Sender<SpeakCommand>,
    event_tx: tokio::sync::broadcast::Sender<SpeakEvent>,
    depth: Arc<AtomicUsize>,
    // std RwLock holding an Arc: a read clones the Arc and drops the guard in the
    // same statement, so the lock is never held across an `.await`. Lets queries
    // read the catalog without an actor round-trip.
    voices: Arc<std::sync::RwLock<Arc<Vec<TtsVoice>>>>,
    disabled_engines: Arc<std::sync::RwLock<Arc<HashSet<EngineId>>>>,
    resolver: Arc<std::sync::RwLock<VoiceAliasResolver>>,
    master_volume_bits: Arc<AtomicU32>,
}

impl SpeakQueueHandle {
    pub async fn send(&self, cmd: SpeakCommand) -> Result<(), SpeakError> {
        self.tx.send(cmd).await.map_err(|_| SpeakError::ActorGone)
    }

    pub fn queue_depth(&self) -> usize {
        self.depth.load(Ordering::Relaxed)
    }

    pub fn available_voices(&self) -> Arc<Vec<TtsVoice>> {
        self.voices
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn disabled_engines(&self) -> Arc<HashSet<EngineId>> {
        self.disabled_engines
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn synthesis_defaults(&self) -> SynthesisDefaults {
        self.resolver
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .defaults
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

    pub fn blocking_send(&self, cmd: SpeakCommand) -> Result<(), SpeakError> {
        self.tx
            .blocking_send(cmd)
            .map_err(|_| SpeakError::ActorGone)
    }

    pub async fn notify_voicegate_active(&self) -> Result<(), SpeakError> {
        self.send(SpeakCommand::VoiceGateActivated).await
    }

    pub async fn notify_voicegate_inactive(&self) -> Result<(), SpeakError> {
        self.send(SpeakCommand::VoiceGateDeactivated).await
    }

    /// Each call returns an independent receiver starting from the next published
    /// event. Callers may wrap this in `tokio_stream::wrappers::BroadcastStream`
    /// to adapt it for use with `iced::Subscription`.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SpeakEvent> {
        self.event_tx.subscribe()
    }
}

pub struct SpeakEventStream(tokio::sync::broadcast::Receiver<SpeakEvent>);

impl SpeakEventStream {
    pub async fn recv(&mut self) -> Result<SpeakEvent, SpeakError> {
        self.0.recv().await.map_err(|_| SpeakError::ActorGone)
    }
}

/// Returns a `SpeakQueueHandle` for command dispatch and a `SpeakEventStream`
/// for UI subscriptions.
pub fn spawn(config: QueueConfig, deps: QueueDeps) -> (SpeakQueueHandle, SpeakEventStream) {
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<SpeakCommand>(256);
    let (event_tx, event_rx) = tokio::sync::broadcast::channel::<SpeakEvent>(256);

    let depth = Arc::new(AtomicUsize::new(0));
    let voices = Arc::new(std::sync::RwLock::new(Arc::new(Vec::<TtsVoice>::new())));
    let disabled_engines = Arc::new(std::sync::RwLock::new(Arc::new(HashSet::<EngineId>::new())));
    let resolver = deps.resolver.clone();
    let master_volume_bits = Arc::new(AtomicU32::new(config.master_volume.to_bits()));

    let event_tx_clone = event_tx.clone();
    let depth_clone = depth.clone();
    let voices_clone = voices.clone();
    let disabled_engines_clone = disabled_engines.clone();
    let master_volume_bits_clone = master_volume_bits.clone();
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
            voices: Arc::new(std::sync::RwLock::new(Arc::new(Vec::new()))),
            disabled_engines: Arc::new(std::sync::RwLock::new(Arc::new(HashSet::new()))),
            resolver: Arc::new(std::sync::RwLock::new(VoiceAliasResolver::new(
                vec![],
                AssignmentStrategy::default(),
                forge_voice::IgnoreProfile::default(),
                SynthesisDefaults::default(),
            ))),
            master_volume_bits: Arc::new(AtomicU32::new(1.0f32.to_bits())),
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
            voices: Arc::new(std::sync::RwLock::new(Arc::new(Vec::new()))),
            disabled_engines: Arc::new(std::sync::RwLock::new(Arc::new(HashSet::new()))),
            resolver: Arc::new(std::sync::RwLock::new(VoiceAliasResolver::new(
                vec![],
                AssignmentStrategy::default(),
                forge_voice::IgnoreProfile::default(),
                SynthesisDefaults::default(),
            ))),
            master_volume_bits: Arc::new(AtomicU32::new(1.0f32.to_bits())),
        };
        let mut sub = handle.subscribe();
        event_tx
            .send(SpeakEvent::Cleared)
            .expect("send must succeed");
        let received = sub.try_recv().expect("receiver must see the event");
        assert!(matches!(received, SpeakEvent::Cleared));
    }
}
