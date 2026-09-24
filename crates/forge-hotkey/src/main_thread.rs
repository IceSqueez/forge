use tokio::sync::mpsc;

const MAIN_THREAD_QUEUE: usize = 16;

type MainThreadJob = Box<dyn FnOnce() + Send>;

pub struct MainThreadHost {
    jobs: mpsc::Receiver<MainThreadJob>,
}

#[derive(Clone)]
pub struct MainThreadLink {
    #[cfg_attr(not(any(target_os = "windows", target_os = "macos")), allow(dead_code))]
    jobs: mpsc::Sender<MainThreadJob>,
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
            job();
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
        let boxed: MainThreadJob = Box::new(move || {
            let _ = reply_tx.send(job());
        });
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
