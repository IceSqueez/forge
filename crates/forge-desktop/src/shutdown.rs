use std::sync::Arc;
use std::time::Duration;

use forge_runtime::{ActionEngineHandle, EventBus, QueueSchedulerHandle, TriggerEvaluatorHandle};
use forge_server::ServerHandle;
use forge_speak_queue::{SpeakCommand, SpeakQueueHandle};

use crate::runtime_handles::RuntimeHandles;

const SETTLE: Duration = Duration::from_millis(40);
const SERVER_STOP_BUDGET: Duration = Duration::from_millis(60);
const SPEAK_STOP_BUDGET: Duration = Duration::from_millis(20);
const FLUSH_BUDGET: Duration = Duration::from_millis(60);
const GRACEFUL_BUDGET: Duration = Duration::from_millis(180);

pub struct ShutdownHandles {
    bus: Arc<EventBus>,
    action_engine: ActionEngineHandle,
    scheduler: QueueSchedulerHandle,
    trigger_evaluator: TriggerEvaluatorHandle,
    server: Option<ServerHandle>,
    speak: Option<SpeakQueueHandle>,
}

impl ShutdownHandles {
    pub fn from_handles(handles: &RuntimeHandles) -> Self {
        Self {
            bus: Arc::clone(&handles.bus),
            action_engine: handles.action_engine.clone(),
            scheduler: handles.scheduler.clone(),
            trigger_evaluator: handles.trigger_evaluator.clone(),
            server: handles.server.clone(),
            speak: handles.speak.clone(),
        }
    }

    pub async fn run_graceful(self) {
        let _ = tokio::time::timeout(GRACEFUL_BUDGET, self.sequence()).await;
    }

    async fn sequence(self) {
        tracing::info!("graceful shutdown: stopping intake");
        self.trigger_evaluator.shutdown();

        if let Some(server) = self.server {
            let _ = tokio::time::timeout(SERVER_STOP_BUDGET, server.stop()).await;
        }

        tokio::time::sleep(SETTLE).await;

        self.scheduler.shutdown();
        self.action_engine.shutdown();

        if let Some(speak) = self.speak {
            let _ = tokio::time::timeout(SPEAK_STOP_BUDGET, speak.send(SpeakCommand::Clear)).await;
        }

        self.bus.shutdown();
        let _ = tokio::time::timeout(FLUSH_BUDGET, self.bus.await_flush()).await;
        tracing::info!("graceful shutdown: event log flushed");
    }
}
