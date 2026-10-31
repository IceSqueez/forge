use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_events::EventPublisher;
use forge_net::Backoff;
use forge_types::{IntegrationId, RecoverMutex};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio::task::AbortHandle;
use tokio_stream::wrappers::ReceiverStream;

use super::status::{PollMetrics, PollStatus, PollStatusBoard};
use super::worker::{self, PollScope, PollWorker};
use crate::PlatformError;
use crate::donation::DonationStream;

const DONATION_BUFFER: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PollControl {
    pub(super) enabled: bool,
    pub(super) generation: u64,
    pub(super) interval: Duration,
}

pub struct DonationPoll<S: Clone + PartialEq, F: Copy + PartialEq> {
    id: IntegrationId,
    control: watch::Sender<PollControl>,
    board: Arc<PollStatusBoard<S, F>>,
    running_poller: Mutex<Option<AbortHandle>>,
}

impl<S: Clone + PartialEq, F: Copy + PartialEq> DonationPoll<S, F> {
    pub fn new(id: IntegrationId, interval: Duration, metrics: PollMetrics<S, F>) -> Self {
        let (control, _) = watch::channel(PollControl {
            enabled: true,
            generation: 0,
            interval,
        });
        let board = Arc::new(PollStatusBoard::new(id.as_str(), interval, metrics));
        Self {
            id,
            control,
            board,
            running_poller: Mutex::new(None),
        }
    }

    pub fn id(&self) -> &IntegrationId {
        &self.id
    }

    pub fn board(&self) -> &Arc<PollStatusBoard<S, F>> {
        &self.board
    }

    pub fn status(&self) -> PollStatus<S, F> {
        self.board.snapshot()
    }

    pub fn attach_publisher(&self, publisher: Arc<dyn EventPublisher>) {
        self.board.attach_publisher(publisher);
    }

    pub fn is_enabled(&self) -> bool {
        self.control.borrow().enabled
    }

    pub fn set_interval(&self, interval: Duration) {
        self.control.send_if_modified(|control| {
            let changed = control.interval != interval;
            control.interval = interval;
            changed
        });
    }

    pub fn restart(&self) {
        self.control.send_modify(|control| {
            control.enabled = true;
            control.generation = control.generation.wrapping_add(1);
        });
    }

    pub fn pause(&self) {
        self.control.send_if_modified(|control| {
            let changed = control.enabled;
            control.enabled = false;
            changed
        });
    }

    pub fn forget_session(&self) {
        self.control.send_modify(|control| {
            control.generation = control.generation.wrapping_add(1);
        });
    }

    pub fn start<W>(&self, worker: W, backoff: Backoff, no_runtime: PlatformError) -> DonationStream
    where
        W: PollWorker<Subject = S, Failure = F>,
        S: Send + Sync + 'static,
        F: Send + Sync + 'static,
    {
        let (sink, receiver) = mpsc::channel(DONATION_BUFFER);
        match Handle::try_current() {
            Ok(runtime) => {
                let scope = PollScope::new(self.control.subscribe(), sink);
                let poller =
                    runtime.spawn(worker::run(worker, scope, backoff, Arc::clone(&self.board)));
                let superseded = self
                    .running_poller
                    .lock_recover()
                    .replace(poller.abort_handle());
                if let Some(superseded) = superseded {
                    superseded.abort();
                }
            }
            Err(_) => {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the fresh channel has room for this single error"
                )]
                let _ = sink.try_send(Err(no_runtime));
            }
        }
        Box::pin(ReceiverStream::new(receiver))
    }
}
