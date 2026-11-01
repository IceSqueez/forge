use std::fmt::Display;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use forge_net::Backoff;
use forge_types::Donation;
use time::OffsetDateTime;
use tokio::sync::{mpsc, watch};
use tracing::warn;

use super::control::PollControl;
use super::status::{PollError, PollStatusBoard, PollerPhase};
use crate::PlatformError;

pub enum CycleError<E> {
    Poll(E),
    SinkClosed,
    Interrupted,
}

impl<E> From<E> for CycleError<E> {
    fn from(error: E) -> Self {
        Self::Poll(error)
    }
}

pub trait PollWorker: Send + 'static {
    type Subject: Clone + PartialEq;
    type Failure: Copy + PartialEq;
    type Error: PollError<Failure = Self::Failure> + Display + Into<PlatformError> + Send;

    const LOG_NAME: &'static str;

    fn forget_session(&mut self);

    fn cycle(
        &mut self,
        scope: &PollScope,
    ) -> impl Future<Output = Result<(), CycleError<Self::Error>>> + Send;

    fn pacing_delay(&self, _error: &Self::Error) -> Option<Duration> {
        None
    }
}

enum Wake {
    Elapsed,
    ControlChanged,
    Closed,
}

type DonationSink = mpsc::Sender<Result<Donation, PlatformError>>;

pub struct PollScope {
    control: watch::Receiver<PollControl>,
    sink: DonationSink,
    active_generation: u64,
}

impl PollScope {
    pub(super) const fn new(control: watch::Receiver<PollControl>, sink: DonationSink) -> Self {
        Self {
            control,
            sink,
            active_generation: 0,
        }
    }

    pub async fn interruptible<T, E>(
        &self,
        work: impl Future<Output = Result<T, E>>,
    ) -> Result<T, CycleError<E>> {
        let generation = self.active_generation;
        let mut control = self.control.clone();
        tokio::select! {
            outcome = work => outcome.map_err(CycleError::Poll),
            _ = control.wait_for(|control| cycle_is_stale(control, generation)) => {
                Err(CycleError::Interrupted)
            }
            () = self.sink.closed() => Err(CycleError::SinkClosed),
        }
    }

    pub async fn emit<S, F, E>(
        &self,
        board: &PollStatusBoard<S, F>,
        mut donations: Vec<Donation>,
        rejected: Vec<E>,
        skipped_log: &str,
    ) -> Result<Option<OffsetDateTime>, CycleError<E>>
    where
        S: Clone + PartialEq + Send,
        F: Copy + PartialEq + Send,
        E: PollError<Failure = F> + Display + Into<PlatformError> + Send,
    {
        donations.sort_by_key(|donation| donation.occurred_at);
        let newest = donations.last().map(|donation| donation.occurred_at);
        for donation in donations {
            self.sink
                .send(Ok(donation))
                .await
                .map_err(|_| CycleError::SinkClosed)?;
        }
        for error in rejected {
            board.record_rejected_donation(&error);
            warn!(error = %error, "{skipped_log}");
            self.sink
                .send(Err(error.into()))
                .await
                .map_err(|_| CycleError::SinkClosed)?;
        }
        Ok(newest)
    }

    async fn wait(&mut self, delay: Option<Duration>) -> Wake {
        let sleep = async {
            match delay {
                Some(delay) => tokio::time::sleep(delay).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            () = sleep => Wake::Elapsed,
            changed = self.control.changed() => {
                if changed.is_ok() { Wake::ControlChanged } else { Wake::Closed }
            }
            () = self.sink.closed() => Wake::Closed,
        }
    }
}

fn cycle_is_stale(control: &PollControl, generation: u64) -> bool {
    !control.enabled || control.generation != generation
}

pub(super) async fn run<W: PollWorker>(
    mut worker: W,
    mut scope: PollScope,
    mut backoff: Backoff,
    board: Arc<PollStatusBoard<W::Subject, W::Failure>>,
) where
    W::Subject: Send + Sync,
    W::Failure: Send + Sync,
{
    let _running = board.poller_started();
    let mut generation = None;
    loop {
        let control = *scope.control.borrow_and_update();
        board.set_poll_interval(control.interval);
        if !control.enabled {
            board.enter(PollerPhase::Paused);
            match scope.wait(None).await {
                Wake::Closed => return,
                Wake::Elapsed | Wake::ControlChanged => continue,
            }
        }
        if generation != Some(control.generation) {
            generation = Some(control.generation);
            scope.active_generation = control.generation;
            worker.forget_session();
            backoff.reset();
        }
        let delay = match worker.cycle(&scope).await {
            Ok(()) => {
                backoff.reset();
                Some(control.interval)
            }
            Err(CycleError::SinkClosed) => return,
            Err(CycleError::Interrupted) => continue,
            Err(CycleError::Poll(error)) => {
                if let Some(delay) = worker.pacing_delay(&error) {
                    Some(delay)
                } else {
                    board.record_failure(&error);
                    warn!(
                        failure = error.failure_label(),
                        error = %error,
                        "{} poll failed",
                        W::LOG_NAME
                    );
                    let needs_user = error.needs_user_action();
                    if needs_user {
                        worker.forget_session();
                    }
                    if scope.sink.send(Err(error.into())).await.is_err() {
                        return;
                    }
                    (!needs_user).then(|| control.interval + backoff.next_delay())
                }
            }
        };
        if let Wake::Closed = scope.wait(delay).await {
            return;
        }
    }
}
