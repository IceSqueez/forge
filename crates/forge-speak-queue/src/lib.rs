mod actor;

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

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
    /// Override the resolved alias.
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
    pub registry: Arc<TtsRegistry>,
    pub resolver: Arc<std::sync::RwLock<VoiceAliasResolver>>,
    pub pipeline: Arc<forge_tts_pipeline::PipelineConfig>,
    pub audio_sink: Arc<dyn forge_audio::AudioSink>,
    pub event_bus: Arc<dyn forge_events::EventPublisher>,
}

/// Handle for dispatching commands to the speak queue actor.
#[derive(Clone)]
pub struct SpeakQueueHandle {
    tx: tokio::sync::mpsc::Sender<SpeakCommand>,
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
}

/// Receive end for the speak queue event broadcast.
pub struct SpeakEventStream(tokio::sync::broadcast::Receiver<SpeakEvent>);

impl SpeakEventStream {
    pub async fn recv(&mut self) -> Result<SpeakEvent, SpeakError> {
        self.0.recv().await.map_err(|_| SpeakError::ActorGone)
    }
}

/// Spawns the speak queue actor.
///
/// Returns a `SpeakQueueHandle` for command dispatch and a `SpeakEventStream`
/// for UI subscriptions.
pub fn spawn(config: QueueConfig, deps: QueueDeps) -> (SpeakQueueHandle, SpeakEventStream) {
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<SpeakCommand>(256);
    let (event_tx, event_rx) = tokio::sync::broadcast::channel::<SpeakEvent>(256);

    let event_tx_clone = event_tx.clone();
    tokio::spawn(async move {
        actor::run_actor(config, deps, cmd_rx, event_tx_clone).await;
    });

    (SpeakQueueHandle { tx: cmd_tx }, SpeakEventStream(event_rx))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn speak_event_serde_roundtrip() {
        let event = SpeakEvent::Enqueued {
            request_id: RequestId("01HWTEST".into()),
            queue_len: 3,
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: SpeakEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, SpeakEvent::Enqueued { queue_len: 3, .. }));
    }

    #[test]
    fn speak_error_display() {
        let err = SpeakError::QueueFull { max: 100 };
        assert!(err.to_string().contains("100"));

        let err2 = SpeakError::ActorGone;
        assert!(err2.to_string().contains("stopped"));
    }

    #[tokio::test]
    async fn handle_send_returns_actor_gone_after_drop() {
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<SpeakCommand>(1);
        drop(cmd_rx);
        let handle = SpeakQueueHandle { tx: cmd_tx };
        let result = handle.send(SpeakCommand::Skip).await;
        assert!(matches!(result, Err(SpeakError::ActorGone)));
    }
}
