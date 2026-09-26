use std::sync::Arc;

use forge_events::DeliveryLane;
use forge_storage::{ActionExecution, ActionRepo, HistoryRepo};
use forge_types::ExecutionContext;
use tokio::sync::mpsc::{self, error::TrySendError};

use crate::bus::EventBus;
use crate::delivery::RUN_HISTORY;
use crate::delivery_loss::{ConsumerLossCounters, DeliveryTier};
use crate::persist_batch::{BATCH_WRITE_TIMEOUT, BatchSink, run_batched};

pub(crate) struct RunRecord {
    pub(crate) context: ExecutionContext,
    pub(crate) execution: Option<ActionExecution>,
}

/// Never waits: a full queue drops the record into drop accounting instead of stalling the run.
#[derive(Clone)]
pub(crate) struct RunHistoryWriter {
    sender: mpsc::Sender<RunRecord>,
    loss: Arc<ConsumerLossCounters>,
}

impl RunHistoryWriter {
    pub(crate) fn spawn(
        bus: &EventBus,
        history: Arc<dyn HistoryRepo>,
        actions: Arc<dyn ActionRepo>,
    ) -> Self {
        let (sender, intake) = mpsc::channel(bus.run_history_capacity());
        let loss = bus.loss_counters(RUN_HISTORY, DeliveryTier::Critical);
        let sink = RunHistorySink {
            history,
            actions,
            loss: Arc::clone(&loss),
            contexts: Vec::new(),
            executions: Vec::new(),
        };
        tokio::spawn(run_batched(
            intake,
            sink,
            bus.batch_policy(),
            bus.flush_ticket(),
        ));
        Self { sender, loss }
    }

    pub(crate) fn record(&self, context: ExecutionContext, execution: Option<ActionExecution>) {
        match self.sender.try_send(RunRecord { context, execution }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => self.loss.lane_dropped(DeliveryLane::Bulk),
            Err(TrySendError::Closed(_)) => {
                tracing::warn!("run history writer has stopped; run not recorded");
                self.loss.unwritten(1);
            }
        }
    }
}

struct RunHistorySink {
    history: Arc<dyn HistoryRepo>,
    actions: Arc<dyn ActionRepo>,
    loss: Arc<ConsumerLossCounters>,
    contexts: Vec<ExecutionContext>,
    executions: Vec<ActionExecution>,
}

impl BatchSink for RunHistorySink {
    type Item = RunRecord;

    async fn flush(&mut self, batch: &mut Vec<RunRecord>) {
        for record in batch.drain(..) {
            self.contexts.push(record.context);
            self.executions.extend(record.execution);
        }

        let runs = self.contexts.len() as u64;
        match tokio::time::timeout(BATCH_WRITE_TIMEOUT, self.history.save_batch(&self.contexts))
            .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::warn!(error = %e, runs, "run history batch save failed");
                self.loss.unwritten(runs);
            }
            Err(_) => {
                tracing::warn!(runs, "run history batch save timed out");
                self.loss.unwritten(runs);
            }
        }
        self.contexts.clear();

        if !self.executions.is_empty() {
            let rows = self.executions.len();
            match tokio::time::timeout(
                BATCH_WRITE_TIMEOUT,
                self.actions.record_executions(&self.executions),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, rows, "action execution stats batch failed");
                }
                Err(_) => tracing::warn!(rows, "action execution stats batch timed out"),
            }
            self.executions.clear();
        }
    }

    fn abandon(&mut self, runs: u64) -> u64 {
        let runs = runs + self.contexts.len() as u64;
        self.contexts.clear();
        self.executions.clear();
        self.loss.unwritten(runs);
        runs
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use forge_storage::action::MockActionRepo;
    use forge_storage::history::MockHistoryRepo;
    use forge_storage::{ExecutionStatus, StorageError};
    use forge_types::{ActionId, ExecutionMetadata, ExecutionOutcome};
    use time::OffsetDateTime;

    use super::*;
    use crate::delivery_loss::LossCount;
    use crate::{Config, NullEventLogRepo};

    fn context() -> ExecutionContext {
        ExecutionContext {
            action_id: ActionId::new(),
            metadata: ExecutionMetadata::QuickAction {
                builtin_id: "obs".to_string(),
                label: "run".to_string(),
            },
            arg_stack_snapshot: BTreeMap::new(),
            started_at: OffsetDateTime::now_utc(),
            completed_at: None,
            telemetry: Vec::new(),
            outcome: ExecutionOutcome::Success,
        }
    }

    fn execution(ctx: &ExecutionContext) -> ActionExecution {
        ActionExecution {
            action_id: ctx.action_id,
            started_at: ctx.started_at,
            duration_ms: 1,
            status: ExecutionStatus::Success,
        }
    }

    fn bus_with_capacity(run_history_capacity: usize) -> Arc<EventBus> {
        let config = Config {
            run_history_capacity,
            ..Config::default()
        };
        EventBus::with_config(Arc::new(NullEventLogRepo), &config)
    }

    fn run_history_loss(bus: &EventBus) -> LossCount {
        bus.loss_report()
            .into_iter()
            .find(|entry| entry.consumer == RUN_HISTORY)
            .map(|entry| entry.loss)
            .unwrap_or_default()
    }

    fn accepting_repos() -> (Arc<dyn HistoryRepo>, Arc<dyn ActionRepo>) {
        let mut history = MockHistoryRepo::new();
        history.expect_save_batch().returning(|_| Ok(()));
        let mut actions = MockActionRepo::new();
        actions.expect_record_executions().returning(|_| Ok(()));
        (Arc::new(history), Arc::new(actions))
    }

    async fn drain(bus: &EventBus) {
        bus.shutdown();
        tokio::time::timeout(std::time::Duration::from_secs(5), bus.await_flush())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_record_that_finds_the_queue_full_is_counted_as_a_bulk_drop() {
        let bus = bus_with_capacity(1);
        let (history, actions) = accepting_repos();
        let writer = RunHistoryWriter::spawn(&bus, history, actions);

        writer.record(context(), None);
        writer.record(context(), None);

        assert_eq!(
            run_history_loss(&bus),
            LossCount {
                bulk_dropped: 1,
                ..LossCount::default()
            }
        );
    }

    #[tokio::test]
    async fn a_record_after_the_writer_stopped_is_counted_unwritten() {
        let bus = bus_with_capacity(8);
        let (history, actions) = accepting_repos();
        let writer = RunHistoryWriter::spawn(&bus, history, actions);
        drain(&bus).await;

        writer.record(context(), None);

        assert_eq!(run_history_loss(&bus).unwritten, 1);
    }

    #[tokio::test]
    async fn runs_queued_together_are_saved_in_one_call_with_their_executions() {
        let saved = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let mut history = MockHistoryRepo::new();
        let saved_calls = Arc::clone(&saved);
        history.expect_save_batch().returning(move |contexts| {
            saved_calls.lock().unwrap().push(contexts.len());
            Ok(())
        });
        let mut actions = MockActionRepo::new();
        let recorded_calls = Arc::clone(&recorded);
        actions
            .expect_record_executions()
            .returning(move |executions| {
                recorded_calls.lock().unwrap().push(executions.to_vec());
                Ok(())
            });
        let bus = bus_with_capacity(8);
        let writer = RunHistoryWriter::spawn(&bus, Arc::new(history), Arc::new(actions));

        let (first, cancelled, third) = (context(), context(), context());
        let expected = vec![vec![execution(&first), execution(&third)]];
        writer.record(first.clone(), Some(execution(&first)));
        writer.record(cancelled, None);
        writer.record(third.clone(), Some(execution(&third)));
        drain(&bus).await;

        assert_eq!(
            (
                saved.lock().unwrap().clone(),
                recorded.lock().unwrap().clone()
            ),
            (vec![3], expected)
        );
    }

    #[tokio::test]
    async fn a_failed_history_save_counts_every_run_of_the_batch_unwritten() {
        let mut history = MockHistoryRepo::new();
        history.expect_save_batch().returning(|_| {
            Err(StorageError::Connection {
                reason: "disk full".to_string(),
            })
        });
        let mut actions = MockActionRepo::new();
        actions.expect_record_executions().returning(|_| Ok(()));
        let bus = bus_with_capacity(8);
        let writer = RunHistoryWriter::spawn(&bus, Arc::new(history), Arc::new(actions));

        writer.record(context(), None);
        writer.record(context(), None);
        drain(&bus).await;

        assert_eq!(run_history_loss(&bus).unwritten, 2);
    }
}
