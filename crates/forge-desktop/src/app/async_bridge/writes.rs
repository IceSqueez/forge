use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Display;
use std::future::Future;
use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use forge_components::{PushToast, STICKY_TOAST, ToastKind};
use gpui::{App, Context, SharedString};
use tokio::runtime::Handle;

use super::errors::ErrorSink;
use super::tasks::on_runtime;
use crate::shared::repo_failure::RepoFailure;

pub(crate) const SLIDER_PERSIST_DEBOUNCE: Duration = Duration::from_millis(400);

pub(crate) struct Debounced {
    generation: Arc<AtomicU64>,
    delay: Duration,
}

impl Debounced {
    pub(crate) fn new(delay: Duration) -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            delay,
        }
    }

    pub(crate) fn schedule_reporting<V, F, E>(
        &self,
        handle: &Handle,
        context: impl Into<SharedString>,
        fut: F,
        sink: ErrorSink,
        cx: &mut Context<V>,
    ) where
        V: 'static,
        F: Future<Output = Result<(), E>> + Send + 'static,
        E: Display + Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.spawn_debounced(handle, context.into(), fut, move |context| {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = tx.send(context);
        });
        cx.spawn(async move |_this, cx| {
            if let Ok(message) = rx.await {
                cx.update(|cx| {
                    sink.report(message.to_string(), cx);
                });
            }
        })
        .detach();
    }

    fn spawn_debounced<F, E>(
        &self,
        handle: &Handle,
        context: SharedString,
        fut: F,
        on_failure: impl FnOnce(SharedString) + Send + 'static,
    ) where
        F: Future<Output = Result<(), E>> + Send + 'static,
        E: Display + Send + 'static,
    {
        let ticket = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation = Arc::clone(&self.generation);
        let delay = self.delay;
        handle.spawn(async move {
            tokio::time::sleep(delay).await;
            if generation.load(Ordering::SeqCst) != ticket {
                return;
            }
            if let Err(e) = fut.await {
                tracing::warn!(error = %e, context = %context, "debounced write failed");
                on_failure(context);
            }
        });
    }
}

#[derive(Default)]
pub(crate) struct Generation(Cell<u64>);

impl Generation {
    pub(crate) fn next(&self) -> u64 {
        let next = self.0.get().wrapping_add(1);
        self.0.set(next);
        next
    }

    pub(crate) fn is_current(&self, ticket: u64) -> bool {
        self.0.get() == ticket
    }
}

pub(crate) fn optimistic<V, S, F, E>(
    handle: &Handle,
    context: &'static str,
    snapshot: S,
    fut: F,
    restore: impl FnOnce(&mut V, S, RepoFailure, &mut Context<V>) + Send + 'static,
    cx: &mut Context<V>,
) where
    V: 'static,
    S: Send + 'static,
    F: Future<Output = Result<(), E>> + Send + 'static,
    E: Into<RepoFailure> + Send + 'static,
{
    let reply = on_runtime(handle, async move { fut.await.map_err(Into::into) });
    cx.spawn(async move |this, cx| {
        if let Ok(Err(failure)) = reply.await {
            tracing::warn!(context, error = %failure, "optimistic persist failed");
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| {
                restore(this, snapshot, failure, cx);
                cx.notify();
            });
        }
    })
    .detach();
}

#[derive(Default)]
pub(crate) enum Loadable<T> {
    #[default]
    Loading,
    Loaded(T),
    Failed(RepoFailure),
}

impl<T> Loadable<T> {
    pub(crate) fn settled<E: Into<RepoFailure>>(result: Result<T, E>) -> Self {
        match result {
            Ok(value) => Loadable::Loaded(value),
            Err(e) => Loadable::Failed(e.into()),
        }
    }

    pub(crate) fn loaded(&self) -> Option<&T> {
        match self {
            Loadable::Loaded(value) => Some(value),
            Loadable::Loading | Loadable::Failed(_) => None,
        }
    }

    pub(crate) fn loaded_mut(&mut self) -> Option<&mut T> {
        match self {
            Loadable::Loaded(value) => Some(value),
            Loadable::Loading | Loadable::Failed(_) => None,
        }
    }

    pub(crate) fn is_loaded(&self) -> bool {
        matches!(self, Loadable::Loaded(_))
    }

    pub(crate) fn failure(&self) -> Option<String> {
        match self {
            Loadable::Failed(reason) => Some(reason.text()),
            Loadable::Loading | Loadable::Loaded(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WriteOutcome {
    Written,
    Superseded,
    Failed(RepoFailure),
}

type SnapshotWrite = std::pin::Pin<Box<dyn Future<Output = Result<(), RepoFailure>> + Send>>;

struct SnapshotJob {
    write: SnapshotWrite,
    settled: tokio::sync::oneshot::Sender<WriteOutcome>,
}

pub(crate) struct SnapshotWriter {
    jobs: tokio::sync::mpsc::UnboundedSender<SnapshotJob>,
    generation: Arc<AtomicU64>,
}

impl SnapshotWriter {
    pub(crate) fn new(handle: &Handle) -> Self {
        let (jobs, queue) = tokio::sync::mpsc::unbounded_channel();
        handle.spawn(run_snapshot_queue(queue));
        Self {
            jobs,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(crate) fn write<V, F, E>(
        &self,
        write: F,
        on_settled: impl FnOnce(&mut V, WriteOutcome, &mut Context<V>) + 'static,
        cx: &mut Context<V>,
    ) where
        V: 'static,
        F: Future<Output = Result<(), E>> + Send + 'static,
        E: Into<RepoFailure> + Send + 'static,
    {
        let ticket = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation = Arc::clone(&self.generation);
        let (settled, outcome) = tokio::sync::oneshot::channel();
        let job = SnapshotJob {
            write: Box::pin(async move { write.await.map_err(Into::into) }),
            settled,
        };
        if self.jobs.send(job).is_err() {
            tracing::warn!("snapshot queue stopped; write dropped");
        }
        cx.spawn(async move |this, cx| {
            let Ok(outcome) = outcome.await else {
                return;
            };
            let outcome = if generation.load(Ordering::SeqCst) == ticket {
                outcome
            } else {
                WriteOutcome::Superseded
            };
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| {
                on_settled(this, outcome, cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn write_on_release<F, E>(
        &self,
        write: F,
        failure: impl Into<SharedString>,
        cx: &mut App,
    ) where
        F: Future<Output = Result<(), E>> + Send + 'static,
        E: Into<RepoFailure> + Send + 'static,
    {
        let failure = failure.into();
        let (settled, outcome) = tokio::sync::oneshot::channel();
        let job = SnapshotJob {
            write: Box::pin(async move { write.await.map_err(Into::into) }),
            settled,
        };
        if self.jobs.send(job).is_err() {
            tracing::warn!("snapshot queue stopped; write dropped");
        }
        cx.spawn(async move |cx| {
            if let Ok(WriteOutcome::Failed(_)) = outcome.await {
                cx.update(|cx| {
                    cx.push_toast_full(ToastKind::Error, failure, None, None, STICKY_TOAST);
                });
            }
        })
        .detach();
    }
}

pub(crate) struct KeyedSnapshotWriters<K> {
    handle: Handle,
    writers: HashMap<K, SnapshotWriter>,
}

impl<K: Eq + Hash> KeyedSnapshotWriters<K> {
    pub(crate) fn new(handle: &Handle) -> Self {
        Self {
            handle: handle.clone(),
            writers: HashMap::new(),
        }
    }

    pub(crate) fn writer(&mut self, key: K) -> &SnapshotWriter {
        let handle = &self.handle;
        self.writers
            .entry(key)
            .or_insert_with(|| SnapshotWriter::new(handle))
    }

    pub(crate) fn write_reporting<V, F, E>(
        &mut self,
        key: K,
        write: F,
        context: impl Into<SharedString>,
        cx: &mut Context<V>,
    ) where
        V: 'static,
        F: Future<Output = Result<(), E>> + Send + 'static,
        E: Into<RepoFailure> + Send + 'static,
    {
        let context = context.into();
        self.writer(key).write(
            write,
            move |_, outcome, cx| {
                if matches!(outcome, WriteOutcome::Failed(_)) {
                    ErrorSink::Toast.report(context.to_string(), cx);
                }
            },
            cx,
        );
    }
}

async fn run_snapshot_queue(mut queue: tokio::sync::mpsc::UnboundedReceiver<SnapshotJob>) {
    while let Some(mut job) = queue.recv().await {
        while let Ok(newer) = queue.try_recv() {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = job.settled.send(WriteOutcome::Superseded);
            job = newer;
        }
        let outcome = match job.write.await {
            Ok(()) => WriteOutcome::Written,
            Err(failure) => {
                tracing::warn!(error = %failure, "snapshot write failed");
                WriteOutcome::Failed(failure)
            }
        };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the view may close before the result arrives"
        )]
        let _ = job.settled.send(outcome);
    }
}
