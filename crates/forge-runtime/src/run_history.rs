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

