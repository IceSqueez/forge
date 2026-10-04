use std::collections::BTreeSet;

use forge_storage::ScheduledRunId;
use tokio::sync::watch;

pub type WaitingRuns = BTreeSet<ScheduledRunId>;

pub struct WaitingForQueueWatch {
    waiting: watch::Receiver<WaitingRuns>,
}

impl WaitingForQueueWatch {
    pub(super) fn new(waiting: watch::Receiver<WaitingRuns>) -> Self {
        Self { waiting }
    }

    pub fn current(&self) -> WaitingRuns {
        self.waiting.borrow().clone()
    }

    pub async fn changed(&mut self) -> Option<WaitingRuns> {
        self.waiting.changed().await.ok()?;
        Some(self.waiting.borrow_and_update().clone())
    }
}
