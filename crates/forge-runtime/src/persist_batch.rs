use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use forge_events::Event;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;

use crate::config::Config;
use crate::delivery::CriticalSubscription;

/// A commit that does not finish inside this bound is abandoned and its rows counted unwritten.
pub(crate) const BATCH_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy)]
pub(crate) struct BatchPolicy {
    pub(crate) max_rows: usize,
    pub(crate) linger: Duration,
}

impl BatchPolicy {
    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            max_rows: config.persist_batch_max_rows.max(1),
            linger: Duration::from_millis(config.persist_batch_linger_ms),
        }
    }
}

pub(crate) trait BatchIntake: Send {
    type Item: Send;

    /// Cancel-safe; `None` once the producer side is gone and nothing is queued.
    fn recv(&mut self) -> impl Future<Output = Option<Self::Item>> + Send;

    fn try_recv(&mut self) -> Option<Self::Item>;
}

pub(crate) trait BatchSink: Send {
    type Item: Send;

    /// Leaves `batch` empty.
    fn flush(&mut self, batch: &mut Vec<Self::Item>) -> impl Future<Output = ()> + Send;

    /// Shutdown gave up on `rows` items this sink never received; returns them plus any it still
    /// holds, all counted as never stored.
    fn abandon(&mut self, rows: u64) -> u64 {
        rows
    }
}

/// Held by one persisting consumer: tells it when to drain or give up, and reports back when done.
pub(crate) struct FlushTicket {
    stop: watch::Receiver<bool>,
    abandon: watch::Receiver<bool>,
    abandoned_rows: Arc<AtomicU64>,
    done: oneshot::Sender<()>,
}

impl FlushTicket {
    pub(crate) fn new(stop: watch::Receiver<bool>, done: oneshot::Sender<()>) -> Self {
        Self {
            stop,
            abandon: watch::channel(false).1,
            abandoned_rows: Arc::new(AtomicU64::new(0)),
            done,
        }
    }

    pub(crate) fn abandonable(
        self,
        abandon: watch::Receiver<bool>,
        abandoned_rows: Arc<AtomicU64>,
    ) -> Self {
        Self {
            abandon,
            abandoned_rows,
            ..self
        }
    }
}

impl BatchIntake for CriticalSubscription {
    type Item = Arc<Event>;

    async fn recv(&mut self) -> Option<Arc<Event>> {
        CriticalSubscription::recv(self).await
    }

    fn try_recv(&mut self) -> Option<Arc<Event>> {
        CriticalSubscription::try_recv(self)
    }
}

impl<T: Send> BatchIntake for mpsc::Receiver<T> {
    type Item = T;

    async fn recv(&mut self) -> Option<T> {
        mpsc::Receiver::recv(self).await
    }

    fn try_recv(&mut self) -> Option<T> {
        mpsc::Receiver::try_recv(self).ok()
    }
}

/// Keeps only the events `map` turns into a row; everything else is consumed and dropped.
pub(crate) struct MappedEvents<F> {
    subscription: CriticalSubscription,
    map: F,
}

impl<F> MappedEvents<F> {
    pub(crate) fn new(subscription: CriticalSubscription, map: F) -> Self {
        Self { subscription, map }
    }
}

impl<T, F> BatchIntake for MappedEvents<F>
where
    T: Send,
    F: FnMut(&Event) -> Option<T> + Send,
{
    type Item = T;

    async fn recv(&mut self) -> Option<T> {
        loop {
            let event = self.subscription.recv().await?;
            if let Some(item) = (self.map)(&event) {
                return Some(item);
            }
        }
    }

    fn try_recv(&mut self) -> Option<T> {
        loop {
            let event = self.subscription.try_recv()?;
            if let Some(item) = (self.map)(&event) {
                return Some(item);
            }
        }
    }
}

/// Commits in batches of at most `policy.max_rows`: whatever is queued (plus what arrives within
/// the linger) goes into the next commit, so a flood commits in full batches.
pub(crate) async fn run_batched<I, S>(
    mut intake: I,
    mut sink: S,
    policy: BatchPolicy,
    ticket: FlushTicket,
) where
    I: BatchIntake,
    S: BatchSink<Item = I::Item>,
{
    let FlushTicket {
        mut stop,
        mut abandon,
        abandoned_rows,
        done,
    } = ticket;
    let mut batch = Vec::with_capacity(policy.max_rows);
    let mut committed = true;
    while committed {
        let first = tokio::select! {
            biased;
            _ = stop.wait_for(|stopping| *stopping) => break,
            item = intake.recv() => item,
        };
        let Some(first) = first else {
            break;
        };
        batch.push(first);
        top_up(&mut intake, &mut batch, policy.max_rows);
        if batch.len() < policy.max_rows && !policy.linger.is_zero() {
            linger(&mut intake, &mut batch, policy, &mut stop).await;
        }
        committed = flush_unless_abandoned(&mut sink, &mut batch, &mut abandon).await;
    }
    while committed {
        top_up(&mut intake, &mut batch, policy.max_rows);
        if batch.is_empty() {
            break;
        }
        committed = flush_unless_abandoned(&mut sink, &mut batch, &mut abandon).await;
    }
    if !committed {
        let mut rows = batch.len() as u64;
        batch.clear();
        while intake.try_recv().is_some() {
            rows += 1;
        }
        abandoned_rows.fetch_add(sink.abandon(rows), Ordering::Relaxed);
    }
    let _ = done.send(());
}

/// `false` once shutdown gives up on this consumer; an in-flight commit is then cancelled.
async fn flush_unless_abandoned<S: BatchSink>(
    sink: &mut S,
    batch: &mut Vec<S::Item>,
    abandon: &mut watch::Receiver<bool>,
) -> bool {
    tokio::select! {
        biased;
        Ok(_) = abandon.wait_for(|abandoning| *abandoning) => false,
        () = sink.flush(batch) => true,
    }
}

fn top_up<I: BatchIntake>(intake: &mut I, batch: &mut Vec<I::Item>, max_rows: usize) {
    while batch.len() < max_rows {
        match intake.try_recv() {
            Some(item) => batch.push(item),
            None => return,
        }
    }
}

async fn linger<I: BatchIntake>(
    intake: &mut I,
    batch: &mut Vec<I::Item>,
    policy: BatchPolicy,
    stop: &mut watch::Receiver<bool>,
) {
    let deadline = Instant::now() + policy.linger;
    while batch.len() < policy.max_rows {
        tokio::select! {
            biased;
            _ = stop.wait_for(|stopping| *stopping) => return,
            () = tokio::time::sleep_until(deadline) => return,
            item = intake.recv() => match item {
                Some(item) => {
                    batch.push(item);
                    top_up(intake, batch, policy.max_rows);
                }
                None => return,
            },
        }
    }
}

