use std::future::Future;
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::task::AbortHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskShutdown {
    Drained,
    Aborted { tasks: usize },
}

#[derive(Clone, Default)]
pub struct TaskOwner {
    tracker: TaskTracker,
    stop: CancellationToken,
    abort: CancellationToken,
}

impl TaskOwner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn<F>(&self, task: F) -> AbortHandle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.tracker.spawn(self.abortable(task)).abort_handle()
    }

    pub fn spawn_on<F>(&self, runtime: &Handle, task: F) -> AbortHandle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.tracker
            .spawn_on(self.abortable(task), runtime)
            .abort_handle()
    }

    pub fn spawn_until_stopped<F>(&self, task: F) -> AbortHandle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn(self.until_stopped(task))
    }

    pub fn spawn_until_stopped_on<F>(&self, runtime: &Handle, task: F) -> AbortHandle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn_on(runtime, self.until_stopped(task))
    }

    pub fn request_stop(&self) {
        self.stop.cancel();
    }

    pub async fn stopped(&self) {
        self.stop.cancelled().await;
    }

    pub async fn shutdown(&self, grace: Duration) -> TaskShutdown {
        self.request_stop();
        self.tracker.close();
        if tokio::time::timeout(grace, self.tracker.wait())
            .await
            .is_ok()
        {
            return TaskShutdown::Drained;
        }
        let tasks = self.tracker.len();
        self.abort.cancel();
        self.tracker.wait().await;
        TaskShutdown::Aborted { tasks }
    }

    fn abortable<F>(&self, task: F) -> impl Future<Output = ()> + Send + 'static
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let abort = self.abort.clone();
        async move {
            tokio::select! {
                biased;
                () = abort.cancelled() => {}
                () = task => {}
            }
        }
    }

    fn until_stopped<F>(&self, task: F) -> impl Future<Output = ()> + Send + 'static
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let stop = self.stop.clone();
        async move {
            tokio::select! {
                biased;
                () = stop.cancelled() => {}
                () = task => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use tokio::sync::Notify;

    use super::*;

    const GRACE: Duration = Duration::from_secs(5);

    struct DropMark(Arc<AtomicBool>);

    impl Drop for DropMark {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    fn drop_mark() -> (DropMark, Arc<AtomicBool>) {
        let dropped = Arc::new(AtomicBool::new(false));
        (DropMark(Arc::clone(&dropped)), dropped)
    }

    async fn settle() {
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn an_owner_with_no_tasks_drains_without_waiting() {
        let started = tokio::time::Instant::now();

        let outcome = TaskOwner::new().shutdown(GRACE).await;

        assert_eq!(outcome, TaskShutdown::Drained);
        assert_eq!(started.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn a_cooperative_task_finishes_its_exit_work_before_shutdown_returns() {
        let owner = TaskOwner::new();
        let (mark, dropped) = drop_mark();
        let exit_work = Duration::from_secs(1);
        owner.spawn({
            let owner = owner.clone();
            async move {
                owner.stopped().await;
                tokio::time::sleep(exit_work).await;
                drop(mark);
            }
        });
        settle().await;
        let started = tokio::time::Instant::now();

        let outcome = owner.shutdown(GRACE).await;

        assert_eq!(outcome, TaskShutdown::Drained);
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(started.elapsed(), exit_work);
    }

    #[tokio::test(start_paused = true)]
    async fn a_task_that_ignores_the_stop_is_dropped_when_the_grace_runs_out() {
        let owner = TaskOwner::new();
        let (mark, dropped) = drop_mark();
        owner.spawn(async move {
            let _mark = mark;
            std::future::pending::<()>().await;
        });
        settle().await;
        let started = tokio::time::Instant::now();

        let outcome = owner.shutdown(GRACE).await;

        assert_eq!(outcome, TaskShutdown::Aborted { tasks: 1 });
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(started.elapsed(), GRACE);
    }

    #[tokio::test(start_paused = true)]
    async fn a_stop_request_drops_only_the_tasks_spawned_until_stopped() {
        let owner = TaskOwner::new();
        let (until_stopped, until_stopped_gone) = drop_mark();
        let (ordinary, ordinary_gone) = drop_mark();
        owner.spawn_until_stopped(async move {
            let _mark = until_stopped;
            std::future::pending::<()>().await;
        });
        owner.spawn(async move {
            let _mark = ordinary;
            std::future::pending::<()>().await;
        });
        settle().await;

        owner.request_stop();
        settle().await;

        assert!(until_stopped_gone.load(Ordering::SeqCst));
        assert!(!ordinary_gone.load(Ordering::SeqCst));
        assert_eq!(
            owner.shutdown(GRACE).await,
            TaskShutdown::Aborted { tasks: 1 }
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_stop_requested_before_anyone_listens_is_not_lost() {
        let owner = TaskOwner::new();
        owner.request_stop();

        let heard = tokio::time::timeout(GRACE, owner.stopped()).await;

        assert!(heard.is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_and_repeated_shutdowns_all_return_drained() {
        let owner = TaskOwner::new();
        let release = Arc::new(Notify::new());
        owner.spawn({
            let owner = owner.clone();
            let release = Arc::clone(&release);
            async move {
                owner.stopped().await;
                release.notified().await;
            }
        });
        let first = tokio::spawn({
            let owner = owner.clone();
            async move { owner.shutdown(GRACE).await }
        });
        let second = tokio::spawn({
            let owner = owner.clone();
            async move { owner.shutdown(GRACE).await }
        });
        settle().await;

        release.notify_one();

        assert_eq!(first.await.unwrap(), TaskShutdown::Drained);
        assert_eq!(second.await.unwrap(), TaskShutdown::Drained);
        assert_eq!(owner.shutdown(GRACE).await, TaskShutdown::Drained);
    }

    #[tokio::test(start_paused = true)]
    async fn a_task_spawned_after_an_expired_shutdown_never_runs() {
        let owner = TaskOwner::new();
        owner.spawn(std::future::pending::<()>());
        settle().await;
        owner.shutdown(Duration::ZERO).await;
        let ran = Arc::new(AtomicBool::new(false));

        owner.spawn({
            let ran = Arc::clone(&ran);
            async move { ran.store(true, Ordering::SeqCst) }
        });
        let outcome = owner.shutdown(GRACE).await;

        assert_eq!(outcome, TaskShutdown::Drained);
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn a_task_spawned_on_a_handle_is_owned_like_any_other() {
        let owner = TaskOwner::new();
        let (mark, dropped) = drop_mark();
        owner.spawn_until_stopped_on(&Handle::current(), async move {
            let _mark = mark;
            std::future::pending::<()>().await;
        });
        settle().await;

        let outcome = owner.shutdown(GRACE).await;

        assert_eq!(outcome, TaskShutdown::Drained);
        assert!(dropped.load(Ordering::SeqCst));
    }
}
