use tokio::sync::mpsc;

const MAIN_THREAD_QUEUE: usize = 16;

trait MainThreadJob: Send {
    fn is_cancelled(&self) -> bool;
    fn run(self: Box<Self>);
}

#[cfg_attr(not(any(target_os = "windows", target_os = "macos")), allow(dead_code))]
struct PendingJob<T, F> {
    reply_tx: tokio::sync::oneshot::Sender<T>,
    job: F,
}

impl<T, F> MainThreadJob for PendingJob<T, F>
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    fn is_cancelled(&self) -> bool {
        self.reply_tx.is_closed()
    }

    fn run(self: Box<Self>) {
        let this = *self;
        let _ = this.reply_tx.send((this.job)());
    }
}

pub struct MainThreadHost {
    jobs: mpsc::Receiver<Box<dyn MainThreadJob>>,
}

#[derive(Clone)]
pub struct MainThreadLink {
    #[cfg_attr(not(any(target_os = "windows", target_os = "macos")), allow(dead_code))]
    jobs: mpsc::Sender<Box<dyn MainThreadJob>>,
}

pub fn main_thread_channel() -> (MainThreadHost, MainThreadLink) {
    let (tx, rx) = mpsc::channel(MAIN_THREAD_QUEUE);
    (MainThreadHost { jobs: rx }, MainThreadLink { jobs: tx })
}

impl MainThreadHost {
    /// Must be polled by the executor of the thread that runs the platform event loop; ends once
    /// every link is dropped.
    pub async fn run(mut self) {
        while let Some(job) = self.jobs.recv().await {
            if !job.is_cancelled() {
                job.run();
            }
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl MainThreadLink {
    pub(crate) async fn run<T: Send + 'static>(
        &self,
        job: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, crate::error::HotkeyError> {
        use crate::error::HotkeyError;

        const MAIN_THREAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let boxed: Box<dyn MainThreadJob> = Box::new(PendingJob { reply_tx, job });
        let round_trip = async {
            self.jobs
                .send(boxed)
                .await
                .map_err(|_| HotkeyError::MainThreadUnavailable)?;
            reply_rx
                .await
                .map_err(|_| HotkeyError::MainThreadUnavailable)
        };
        tokio::time::timeout(MAIN_THREAD_TIMEOUT, round_trip)
            .await
            .map_err(|_| HotkeyError::MainThreadUnavailable)?
    }
}

#[cfg(all(test, any(target_os = "windows", target_os = "macos")))]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::error::HotkeyError;

    #[test]
    fn a_job_sent_through_the_link_runs_on_the_thread_that_drives_the_host() {
        let (host, link) = main_thread_channel();
        let (host_thread_tx, host_thread_rx) = std::sync::mpsc::channel();
        let host_thread = std::thread::spawn(move || {
            host_thread_tx.send(std::thread::current().id()).unwrap();
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(host.run());
        });
        let host_thread_id = host_thread_rx.recv().unwrap();

        let ran_on = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(link.run(|| std::thread::current().id()))
            .unwrap();

        assert_eq!(ran_on, host_thread_id);
        drop(link);
        host_thread.join().unwrap();
    }

    #[tokio::test]
    async fn a_dropped_host_fails_the_job_with_main_thread_unavailable() {
        let (host, link) = main_thread_channel();
        drop(host);

        let outcome = link.run(|| ()).await;

        assert!(matches!(outcome, Err(HotkeyError::MainThreadUnavailable)));
    }

    #[tokio::test(start_paused = true)]
    async fn a_host_that_never_runs_the_job_fails_it_at_the_deadline() {
        let (_host, link) = main_thread_channel();

        let outcome = link.run(|| ()).await;

        assert!(matches!(outcome, Err(HotkeyError::MainThreadUnavailable)));
    }

    #[tokio::test(start_paused = true)]
    async fn a_job_that_timed_out_never_runs_once_the_host_catches_up() {
        let (host, link) = main_thread_channel();
        let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let outcome = link
            .run({
                let ran = std::sync::Arc::clone(&ran);
                move || ran.store(true, std::sync::atomic::Ordering::SeqCst)
            })
            .await;
        assert!(matches!(outcome, Err(HotkeyError::MainThreadUnavailable)));

        drop(link);
        host.run().await;

        assert!(
            !ran.load(std::sync::atomic::Ordering::SeqCst),
            "a register the caller already reported as failed still reached the OS"
        );
    }
}
