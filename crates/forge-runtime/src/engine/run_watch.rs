use forge_registry::CancelSignal;
use forge_types::ExecutionOutcome;
use tokio::sync::oneshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEnd {
    Finished(ExecutionOutcome),
    NotStarted,
}

#[derive(Clone)]
pub struct RunCanceller(CancelSignal);

impl RunCanceller {
    pub fn cancel(&self) {
        self.0.cancel();
    }
}

pub struct RunCompletion(oneshot::Receiver<RunEnd>);

impl RunCompletion {
    pub async fn finished(self) -> RunEnd {
        self.0.await.unwrap_or(RunEnd::NotStarted)
    }
}

pub struct WatchedRun {
    pub canceller: RunCanceller,
    pub completion: RunCompletion,
}

pub(crate) struct RunWatch {
    pub(crate) cancel: CancelSignal,
    pub(crate) report: oneshot::Sender<RunEnd>,
}

pub(crate) fn watch_run() -> (RunWatch, WatchedRun) {
    let cancel = CancelSignal::new();
    let (report, completion) = oneshot::channel();
    (
        RunWatch {
            cancel: cancel.clone(),
            report,
        },
        WatchedRun {
            canceller: RunCanceller(cancel),
            completion: RunCompletion(completion),
        },
    )
}
