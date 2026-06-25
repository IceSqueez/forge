mod actor;
pub mod filters;

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use filters::{
    FilterMappingError, PipelineConfigHandle, build_config_lenient, build_config_strict,
};
pub use forge_tts_core::TtsError;
use forge_tts_core::{EngineId, TtsRegistry, VoiceId};
use forge_voice::{AliasId, VoiceAliasResolver};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub String);

impl RequestId {
    pub fn new() -> Self {
        Self(ulid::Ulid::new().to_string())
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
    /// Bits/sub/channel-point rewards — head-of-normal-queue but behind other High.
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
    pub source_event_id: forge_types::EventId,
}

#[derive(Debug)]
pub enum SpeakCommand {
    Enqueue(SpeakRequest),
    Skip,
    Clear,
    Pause,
    Resume,
    Replay,
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
    },
    Started {
        request_id: RequestId,
        voice_id: VoiceId,
        engine_id: EngineId,
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
    // std::sync::RwLock here, not tokio — guard never crosses await.
    pub registry: Arc<std::sync::RwLock<TtsRegistry>>,
    pub resolver: Arc<std::sync::RwLock<VoiceAliasResolver>>,
    pub pipeline: PipelineConfigHandle,
    pub audio_sink: Arc<dyn forge_audio::AudioSink>,
    pub event_bus: Arc<dyn forge_events::EventPublisher>,
}

#[derive(Clone)]
pub struct SpeakQueueHandle {
    tx: tokio::sync::mpsc::Sender<SpeakCommand>,
    event_tx: tokio::sync::broadcast::Sender<SpeakEvent>,
}

impl SpeakQueueHandle {
    pub async fn send(&self, cmd: SpeakCommand) -> Result<(), SpeakError> {
        self.tx.send(cmd).await.map_err(|_| SpeakError::ActorGone)
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

    let event_tx_clone = event_tx.clone();
    tokio::spawn(async move {
        actor::run_actor(config, deps, cmd_rx, event_tx_clone).await;
    });

    (
        SpeakQueueHandle {
            tx: cmd_tx,
            event_tx,
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
        };
        let mut sub = handle.subscribe();
        event_tx
            .send(SpeakEvent::Cleared)
            .expect("send must succeed");
        let received = sub.try_recv().expect("receiver must see the event");
        assert!(matches!(received, SpeakEvent::Cleared));
    }
}
