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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use super::*;

    const BOUND: Duration = Duration::from_millis(10);

    async fn settle() {
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn stop_returns_only_once_the_task_marks_itself_stopped() {
        let (stop, listener) = task_stop();
        let stopping = tokio::spawn(stop.stop());
        listener.requested().await;
        settle().await;
        let finished_before_mark = stopping.is_finished();

        listener.mark_stopped();

        let returned = tokio::time::timeout(BOUND, stopping).await.is_ok();
        assert_eq!((finished_before_mark, returned), (false, true));
    }

    #[tokio::test(start_paused = true)]
    async fn a_stop_requested_before_the_task_listens_is_not_lost() {
        let (stop, listener) = task_stop();
        let _stopping = tokio::spawn(stop.stop());
        settle().await;

        let heard = tokio::time::timeout(BOUND, listener.requested()).await;

        assert!(heard.is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_and_repeated_stops_all_return_once_the_task_stopped() {
        let (stop, listener) = task_stop();
        let first = tokio::spawn(stop.clone().stop());
        let second = tokio::spawn(stop.clone().stop());
        listener.requested().await;
        listener.mark_stopped();
        drop(listener);

        let returned = tokio::time::timeout(BOUND, async {
            first.await.unwrap();
            second.await.unwrap();
            stop.clone().stop().await;
            stop.stop().await;
        })
        .await;

        assert!(returned.is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn stop_returns_when_the_task_ends_without_marking_itself_stopped() {
        let (stop, listener) = task_stop();
        let stopping = tokio::spawn(stop.stop());
        settle().await;

        drop(listener);

        assert!(tokio::time::timeout(BOUND, stopping).await.is_ok());
    }
}
