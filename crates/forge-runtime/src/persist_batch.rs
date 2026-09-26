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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    const DEADLINE: Duration = Duration::from_secs(5);

    #[derive(Clone, Default)]
    struct RecordingSink {
        batches: Arc<Mutex<Vec<Vec<u32>>>>,
    }

    impl RecordingSink {
        fn sizes(&self) -> Vec<usize> {
            self.batches.lock().unwrap().iter().map(Vec::len).collect()
        }
    }

    impl BatchSink for RecordingSink {
        type Item = u32;

        async fn flush(&mut self, batch: &mut Vec<u32>) {
            self.batches.lock().unwrap().push(std::mem::take(batch));
        }
    }

    fn default_policy() -> BatchPolicy {
        BatchPolicy::from_config(&Config::default())
    }

    fn ticket() -> (watch::Sender<bool>, FlushTicket, oneshot::Receiver<()>) {
        let (stop, stopping) = watch::channel(false);
        let (done, finished) = oneshot::channel();
        (stop, FlushTicket::new(stopping, done), finished)
    }

    async fn queued(rows: usize) -> (mpsc::Sender<u32>, mpsc::Receiver<u32>) {
        let (tx, rx) = mpsc::channel(rows.max(1));
        for row in 0..rows {
            tx.send(row as u32).await.unwrap();
        }
        (tx, rx)
    }

    #[tokio::test]
    async fn a_queued_backlog_commits_in_batches_capped_at_max_rows() {
        let max = default_policy().max_rows;
        for (depth, expected) in [
            (1, vec![1]),
            (max - 1, vec![max - 1]),
            (max, vec![max]),
            (max + 1, vec![max, 1]),
        ] {
            let (tx, rx) = queued(depth).await;
            drop(tx);
            let sink = RecordingSink::default();
            let (_stop, ticket, _done) = ticket();

            tokio::time::timeout(
                DEADLINE,
                run_batched(rx, sink.clone(), default_policy(), ticket),
            )
            .await
            .unwrap();

            assert_eq!(sink.sizes(), expected, "queue depth {depth}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn rows_arriving_within_the_linger_share_a_commit_and_later_rows_do_not() {
        let policy = BatchPolicy {
            max_rows: 16,
            linger: Duration::from_millis(50),
        };
        let (tx, rx) = mpsc::channel(16);
        let sink = RecordingSink::default();
        let (_stop, ticket, done) = ticket();
        tx.send(1).await.unwrap();
        tokio::spawn(run_batched(rx, sink.clone(), policy, ticket));

        tokio::time::sleep(Duration::from_millis(10)).await;
        tx.send(2).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        tx.send(3).await.unwrap();
        drop(tx);
        done.await.unwrap();

        assert_eq!(sink.sizes(), vec![2, 1]);
    }

    #[tokio::test]
    async fn a_stop_drains_every_queued_row_in_capped_batches_then_reports_done() {
        let max = default_policy().max_rows;
        let (_tx, rx) = queued(2 * max + 1).await;
        let sink = RecordingSink::default();
        let (stop, ticket, done) = ticket();
        stop.send_replace(true);

        tokio::time::timeout(
            DEADLINE,
            run_batched(rx, sink.clone(), default_policy(), ticket),
        )
        .await
        .unwrap();

        assert_eq!(
            (sink.sizes(), done.await.is_ok()),
            (vec![max, max, 1], true)
        );
    }

    #[tokio::test]
    async fn a_stop_while_idle_ends_the_writer_though_its_producer_is_alive() {
        let (_tx, rx) = queued(0).await;
        let sink = RecordingSink::default();
        let (stop, ticket, done) = ticket();
        let writer = tokio::spawn(run_batched(rx, sink.clone(), default_policy(), ticket));
        tokio::task::yield_now().await;

        stop.send_replace(true);

        assert!(tokio::time::timeout(DEADLINE, done).await.unwrap().is_ok());
        writer.await.unwrap();
        assert!(sink.sizes().is_empty());
    }

    #[tokio::test]
    async fn a_stop_during_the_linger_commits_the_pending_row_at_once() {
        let policy = BatchPolicy {
            max_rows: 16,
            linger: Duration::from_secs(60),
        };
        let (_tx, rx) = queued(1).await;
        let sink = RecordingSink::default();
        let (stop, ticket, done) = ticket();
        tokio::spawn(run_batched(rx, sink.clone(), policy, ticket));
        tokio::task::yield_now().await;

        stop.send_replace(true);

        assert!(tokio::time::timeout(DEADLINE, done).await.unwrap().is_ok());
        assert_eq!(sink.sizes(), vec![1]);
    }

    struct StuckSink {
        committing: Arc<tokio::sync::Notify>,
        handed_back: Arc<Mutex<Option<u64>>>,
        held_by_sink: u64,
    }

    impl BatchSink for StuckSink {
        type Item = u32;

        async fn flush(&mut self, _batch: &mut Vec<u32>) {
            self.committing.notify_one();
            std::future::pending::<()>().await;
        }

        fn abandon(&mut self, rows: u64) -> u64 {
            *self.handed_back.lock().unwrap() = Some(rows);
            rows + self.held_by_sink
        }
    }

    #[tokio::test]
    async fn an_abandon_during_a_stuck_commit_counts_the_batch_the_queue_and_what_the_sink_holds() {
        let policy = BatchPolicy {
            max_rows: 2,
            linger: Duration::ZERO,
        };
        let (_tx, rx) = queued(5).await;
        let committing = Arc::new(tokio::sync::Notify::new());
        let handed_back = Arc::new(Mutex::new(None));
        let sink = StuckSink {
            committing: Arc::clone(&committing),
            handed_back: Arc::clone(&handed_back),
            held_by_sink: 7,
        };
        let (abandon, abandoning) = watch::channel(false);
        let given_up = Arc::new(AtomicU64::new(0));
        let (stop, ticket, done) = ticket();
        let ticket = ticket.abandonable(abandoning, Arc::clone(&given_up));
        tokio::spawn(run_batched(rx, sink, policy, ticket));
        committing.notified().await;

        stop.send_replace(true);
        abandon.send_replace(true);
        tokio::time::timeout(DEADLINE, done).await.unwrap().unwrap();

        assert_eq!(
            (
                *handed_back.lock().unwrap(),
                given_up.load(Ordering::Relaxed)
            ),
            (Some(5), 12)
        );
    }
}
