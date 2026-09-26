use std::sync::Arc;
use std::time::Duration;

use forge_runtime::{ActionEngineHandle, EventBus, QueueSchedulerHandle, TriggerEvaluatorHandle};
use forge_server::ServerHandle;
use forge_speak_queue::{SpeakCommand, SpeakQueueHandle};
use forge_storage::DataProvider;

use crate::runtime_handles::RuntimeHandles;

const SETTLE: Duration = Duration::from_millis(30);
const SERVER_STOP_BUDGET: Duration = Duration::from_millis(50);
const SPEAK_STOP_BUDGET: Duration = Duration::from_millis(20);
const FLUSH_BUDGET: Duration = Duration::from_millis(60);
const ABANDON_BUDGET: Duration = Duration::from_millis(10);
const STORAGE_CLOSE_BUDGET: Duration = Duration::from_millis(20);
/// Must stay under the UI shell's quit timeout (200 ms); it stops waiting on quit tasks past that.
const GRACEFUL_BUDGET: Duration = SETTLE
    .saturating_add(SERVER_STOP_BUDGET)
    .saturating_add(SPEAK_STOP_BUDGET)
    .saturating_add(FLUSH_BUDGET)
    .saturating_add(ABANDON_BUDGET)
    .saturating_add(STORAGE_CLOSE_BUDGET);

pub struct ShutdownHandles {
    bus: Arc<EventBus>,
    action_engine: ActionEngineHandle,
    scheduler: QueueSchedulerHandle,
    trigger_evaluator: TriggerEvaluatorHandle,
    server: Option<ServerHandle>,
    speak: Option<SpeakQueueHandle>,
    hotkey: Option<Arc<forge_hotkey::HotkeyClient>>,
    storage: Arc<dyn DataProvider>,
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
            hotkey: handles.hotkey_client.clone(),
            storage: Arc::clone(&handles.backend),
        }
    }

    pub async fn run_graceful(self) {
        let _ = tokio::time::timeout(GRACEFUL_BUDGET, self.sequence()).await;
    }

    async fn sequence(self) {
        if let Some(hotkey) = &self.hotkey {
            // The evaluator drains its backlog on cancel, so a release published here still fires.
            hotkey.release_open_holds();
        }

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
        if tokio::time::timeout(FLUSH_BUDGET, self.bus.await_flush())
            .await
            .is_ok()
        {
            tracing::info!("graceful shutdown: event log flushed");
        } else {
            match tokio::time::timeout(ABANDON_BUDGET, self.bus.abandon_flush()).await {
                Ok(rows) => tracing::warn!(
                    rows,
                    "graceful shutdown: flush budget ran out; queued rows left unwritten"
                ),
                Err(_) => tracing::warn!(
                    "graceful shutdown: flush budget ran out; unwritten rows could not be counted"
                ),
            }
        }

        let _ = tokio::time::timeout(STORAGE_CLOSE_BUDGET, self.storage.shutdown()).await;
        tracing::info!("graceful shutdown: storage closed");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::io::Write;
    use std::sync::Mutex;

    use forge_events::{Event, EventSource};
    use forge_registry::{SubActionRegistry, TriggerRegistry};
    use forge_runtime::{
        ActionCancelRegistry, Config, QueueScheduler, spawn_action_engine, spawn_trigger_evaluator,
    };
    use forge_storage::action::MockActionRepo;
    use forge_storage::history::MockHistoryRepo;
    use forge_storage::{EventLogRepo, StorageError};
    use forge_types::EventId;
    use time::OffsetDateTime;

    use super::*;
    use crate::test_support::{stub_catalog, test_backend};

    struct StuckEventLog;

    #[async_trait::async_trait]
    impl EventLogRepo for StuckEventLog {
        async fn insert(&self, _: &Event) -> Result<(), StorageError> {
            std::future::pending().await
        }

        async fn get(&self, _: EventId) -> Result<Option<Event>, StorageError> {
            Ok(None)
        }

        async fn recent(&self, _: usize) -> Result<Vec<Event>, StorageError> {
            Ok(Vec::new())
        }

        async fn recent_since(
            &self,
            _: usize,
            _: Option<EventId>,
        ) -> Result<Vec<Event>, StorageError> {
            Ok(Vec::new())
        }

        async fn prune_before(&self, _: OffsetDateTime) -> Result<u64, StorageError> {
            Ok(0)
        }
    }

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Captured {
        fn lines_containing(&self, needle: &str) -> Vec<String> {
            String::from_utf8(self.0.lock().unwrap().clone())
                .unwrap()
                .lines()
                .filter(|line| line.contains(needle))
                .map(str::to_owned)
                .collect()
        }
    }

    fn handles_over(bus: Arc<EventBus>) -> ShutdownHandles {
        let catalog = stub_catalog();
        let action_engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&catalog),
            Arc::new(MockActionRepo::new()),
            Arc::new(MockHistoryRepo::new()),
            Arc::new(SubActionRegistry::new()),
            Arc::new(ActionCancelRegistry::new()),
        );
        let scheduler = QueueScheduler::spawn(action_engine.clone(), Arc::clone(&bus), Vec::new());
        let trigger_evaluator = spawn_trigger_evaluator(
            Arc::clone(&bus),
            Arc::new(TriggerRegistry::new()),
            catalog,
            scheduler.clone(),
            Config::default(),
        );
        ShutdownHandles {
            bus,
            action_engine,
            scheduler,
            trigger_evaluator,
            server: None,
            speak: None,
            hotkey: None,
            storage: test_backend().0,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_flush_that_outlasts_its_budget_logs_the_rows_left_unwritten_once() {
        let captured = Captured::default();
        let sink = captured.clone();
        let _logs = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_ansi(false)
                .with_writer(move || sink.clone())
                .finish(),
        );
        let bus = EventBus::new(Arc::new(StuckEventLog));
        EventBus::spawn_flush_task(Arc::clone(&bus));
        let handles = handles_over(Arc::clone(&bus));
        for kind in ["a", "b", "c"] {
            bus.publish(Event::new(EventSource::Core, kind, serde_json::Value::Null));
        }

        handles.run_graceful().await;

        let lines = captured.lines_containing("flush budget ran out");
        assert!(
            matches!(lines.as_slice(), [only] if only.contains("rows=3")),
            "{lines:?}"
        );
    }

    #[test]
    fn every_step_budget_up_to_the_unwritten_row_count_fits_inside_the_graceful_budget() {
        let worst_case =
            SETTLE + SERVER_STOP_BUDGET + SPEAK_STOP_BUDGET + FLUSH_BUDGET + ABANDON_BUDGET;

        assert!(
            worst_case <= GRACEFUL_BUDGET,
            "{worst_case:?} of steps before the count is logged, {GRACEFUL_BUDGET:?} allowed"
        );
    }
}
