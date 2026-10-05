use std::sync::Arc;

use tokio::sync::{Notify, watch};

#[derive(Clone)]
pub struct TaskStop {
    requested: Arc<Notify>,
    stopped: watch::Receiver<bool>,
}

impl TaskStop {
    pub async fn stop(mut self) {
        self.requested.notify_one();
        let _ = self.stopped.wait_for(|stopped| *stopped).await;
    }
}

pub(crate) struct StopListener {
    requested: Arc<Notify>,
    stopped: watch::Sender<bool>,
}

impl StopListener {
    pub(crate) async fn requested(&self) {
        self.requested.notified().await;
    }

    pub(crate) fn mark_stopped(&self) {
        self.stopped.send_replace(true);
    }
}

pub(crate) fn task_stop() -> (TaskStop, StopListener) {
    let requested = Arc::new(Notify::new());
    let (stopped_tx, stopped) = watch::channel(false);
    (
        TaskStop {
            requested: Arc::clone(&requested),
            stopped,
        },
        StopListener {
            requested,
            stopped: stopped_tx,
        },
    )
}
