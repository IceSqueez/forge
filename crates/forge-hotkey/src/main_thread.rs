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
