use std::sync::Arc;

use forge_events::Event;
use forge_storage::EventLogRepo;

use crate::delivery_loss::ConsumerLossCounters;
use crate::persist_batch::{BATCH_WRITE_TIMEOUT, BatchSink};

pub(crate) struct EventLogSink {
    repo: Arc<dyn EventLogRepo>,
    loss: Arc<ConsumerLossCounters>,
}

impl EventLogSink {
    pub(crate) fn new(repo: Arc<dyn EventLogRepo>, loss: Arc<ConsumerLossCounters>) -> Self {
        Self { repo, loss }
    }
}

impl BatchSink for EventLogSink {
    type Item = Arc<Event>;

    async fn flush(&mut self, batch: &mut Vec<Arc<Event>>) {
        let rows = batch.len() as u64;
        match tokio::time::timeout(BATCH_WRITE_TIMEOUT, self.repo.insert_batch(batch)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::warn!(error = %e, rows, "event_log batch insert failed; events not persisted");
                self.loss.unwritten(rows);
            }
            Err(_) => {
                tracing::warn!(
                    rows,
                    "event_log batch insert timed out; events not persisted"
                );
                self.loss.unwritten(rows);
            }
        }
        batch.clear();
    }

    fn abandon(&mut self, rows: u64) -> u64 {
        self.loss.unwritten(rows);
        rows
    }
}
