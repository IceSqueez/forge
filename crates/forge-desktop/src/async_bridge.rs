use std::cell::Cell;
use std::fmt::Display;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use forge_components::ToastKind;
use forge_events::{Event, EventsError};
use forge_runtime::{EventBus, EventSubscription};
use gpui::{App, AsyncApp, Context, Entity, SharedString};
use tokio::runtime::Handle;

use crate::toasts::PushToast;

const BRIDGE_DRAIN_CAP: usize = 128;

pub const SLIDER_PERSIST_DEBOUNCE: Duration = Duration::from_millis(400);

pub enum EventBatch {
    Ready(Vec<Event>),
    Closed,
}

pub async fn recv_event_batch(sub: &mut EventSubscription) -> EventBatch {
    let first = loop {
        match sub.recv().await {
            Ok(event) => break event,
            Err(EventsError::LaggingReceiver) => {
                tracing::warn!("event bridge lagged; dropped events");
                continue;
            }
            Err(_) => return EventBatch::Closed,
        }
    };
    let mut batch = Vec::with_capacity(1);
    batch.push(first);
    while batch.len() < BRIDGE_DRAIN_CAP {
        match sub.try_recv() {
            Ok(Some(event)) => batch.push(event),
            _ => break,
        }
    }
    EventBatch::Ready(batch)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BridgeFlow {
    Continue,
    Stop,
}

/// Drains `bus` batches into `apply` until the bus closes or `apply` returns `Stop`; a lagging receiver warns and keeps receiving (broadcast semantics).
pub async fn drain_events<F>(bus: &EventBus, cx: &mut AsyncApp, mut apply: F)
where
    F: FnMut(&[Event], &mut AsyncApp) -> BridgeFlow,
{
    let mut sub = bus.subscribe();
    while let EventBatch::Ready(batch) = recv_event_batch(&mut sub).await {
        if apply(&batch, cx) == BridgeFlow::Stop {
            break;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ErrorSink {
    Toast,
    Banner,
    Silent,
}

impl ErrorSink {
    pub fn report(self, message: String, cx: &mut App) -> Option<String> {
        match self {
            ErrorSink::Toast => {
                cx.push_toast(ToastKind::Error, message);
                None
            }
            ErrorSink::Banner => Some(message),
            ErrorSink::Silent => None,
        }
    }
}

pub fn report_failure<V, F, E>(
    handle: &Handle,
    fut: F,
    sink: ErrorSink,
    context: impl Into<SharedString>,
    cx: &mut Context<V>,
) where
    V: 'static,
    F: Future<Output = Result<(), E>> + Send + 'static,
    E: Display + Send + 'static,
{
    let context = context.into();
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        if let Err(e) = fut.await {
            tracing::warn!(error = %e, context = %context, "async operation failed");
            let _ = tx.send(context);
        }
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

pub async fn open_path(target: impl AsRef<std::ffi::OsStr> + Send + 'static) -> Result<(), String> {
    tokio::task::spawn_blocking(move || open::that(target))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

pub fn open_external<V>(
    handle: &Handle,
    target: impl Into<String>,
    sink: ErrorSink,
    context: impl Into<SharedString>,
    cx: &mut Context<V>,
) where
    V: 'static,
{
    let target = target.into();
    report_failure(handle, open_path(target), sink, context, cx);
}

pub const DIALOG_CANCELLED: &str = "dialog cancelled";

pub struct DialogFilter {
    pub name: String,
    pub extensions: &'static [&'static str],
}

pub async fn pick_file(filter: Option<DialogFilter>) -> Result<PathBuf, String> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(filter) = filter {
        dialog = dialog.add_filter(filter.name, filter.extensions);
    }
    dialog
        .pick_file()
        .await
        .map(|handle| handle.path().to_path_buf())
        .ok_or_else(|| DIALOG_CANCELLED.to_owned())
}

pub async fn pick_folder() -> Result<PathBuf, String> {
    rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .map(|handle| handle.path().to_path_buf())
        .ok_or_else(|| DIALOG_CANCELLED.to_owned())
}

pub async fn save_file(
    filter: Option<DialogFilter>,
    default_name: Option<String>,
) -> Result<PathBuf, String> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(filter) = filter {
        dialog = dialog.add_filter(filter.name, filter.extensions);
    }
    if let Some(name) = default_name {
        dialog = dialog.set_file_name(name);
    }
    dialog
        .save_file()
        .await
        .map(|handle| handle.path().to_path_buf())
        .ok_or_else(|| DIALOG_CANCELLED.to_owned())
}

pub fn spawn_dialog<V, Fut, T>(
    handle: &Handle,
    dialog: Fut,
    on_result: impl FnOnce(&mut V, Result<T, String>, &mut Context<V>) + Send + 'static,
    cx: &mut Context<V>,
) where
    V: 'static,
    T: Send + 'static,
    Fut: Future<Output = Result<T, String>> + Send + 'static,
{
    run_async(handle, dialog, on_result, cx);
}

/// Runs `fut` on `handle`, then applies its output to the calling view; a dropped view or a dropped task silently no-ops (the view is held weakly).
pub fn run_async<V, Fut, T>(
    handle: &Handle,
    fut: Fut,
    apply: impl FnOnce(&mut V, T, &mut Context<V>) + Send + 'static,
    cx: &mut Context<V>,
) where
    V: 'static,
    T: Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        let _ = tx.send(fut.await);
    });
    cx.spawn(async move |this, cx| {
        if let Ok(result) = rx.await {
            let _ = this.update(cx, |this, cx| apply(this, result, cx));
        }
    })
    .detach();
}

/// Like `run_async` but from an `&mut App` with an explicit `view`; the strong `Entity` keeps the view alive until the future resolves.
pub fn run_async_entity<V, Fut, T>(
    handle: &Handle,
    view: Entity<V>,
    fut: Fut,
    apply: impl FnOnce(&mut V, T, &mut Context<V>) + Send + 'static,
    app: &mut App,
) where
    V: 'static,
    T: Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        let _ = tx.send(fut.await);
    });
    app.spawn(async move |cx| {
        if let Ok(result) = rx.await {
            view.update(cx, |this, cx| apply(this, result, cx));
        }
    })
    .detach();
}

/// Runs blocking `work` on `handle`'s blocking pool, then applies its output to the calling view (held weakly).
pub fn run_blocking<V, Work, T>(
    handle: &Handle,
    work: Work,
    apply: impl FnOnce(&mut V, T, &mut Context<V>) + Send + 'static,
    cx: &mut Context<V>,
) where
    V: 'static,
    T: Send + 'static,
    Work: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn_blocking(move || {
        let _ = tx.send(work());
    });
    cx.spawn(async move |this, cx| {
        if let Ok(result) = rx.await {
            let _ = this.update(cx, |this, cx| apply(this, result, cx));
        }
    })
    .detach();
}

pub struct Debounced {
    generation: Arc<AtomicU64>,
    delay: Duration,
}

impl Debounced {
    pub fn new(delay: Duration) -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            delay,
        }
    }

    /// Runs `fut` after `delay` of quiet, superseding any still-pending write so only the last value of a burst reaches disk (last-write-wins). The write is dispatched on `handle`, so it outlives a drop of the calling view mid-debounce; failures log (no view is guaranteed to survive to toast on).
    pub fn schedule<F, E>(&self, handle: &Handle, context: impl Into<SharedString>, fut: F)
    where
        F: Future<Output = Result<(), E>> + Send + 'static,
        E: Display + Send + 'static,
    {
        let ticket = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation = Arc::clone(&self.generation);
        let delay = self.delay;
        let context = context.into();
        handle.spawn(async move {
            tokio::time::sleep(delay).await;
            if generation.load(Ordering::SeqCst) != ticket {
                return;
            }
            if let Err(e) = fut.await {
                tracing::warn!(error = %e, context = %context, "debounced write failed");
            }
        });
    }
}

#[derive(Default)]
pub struct Generation(Cell<u64>);

impl Generation {
    pub fn next(&self) -> u64 {
        let next = self.0.get().wrapping_add(1);
        self.0.set(next);
        next
    }

    /// True only for the most recently issued ticket; a stale async result (an older request that resolved after a newer one) compares false and must be discarded.
    pub fn is_current(&self, ticket: u64) -> bool {
        self.0.get() == ticket
    }
}

pub fn optimistic<V, S, F, E>(
    handle: &Handle,
    snapshot: S,
    fut: F,
    restore: impl FnOnce(&mut V, S, String, &mut Context<V>) + Send + 'static,
    cx: &mut Context<V>,
) where
    V: 'static,
    S: Send + 'static,
    F: Future<Output = Result<(), E>> + Send + 'static,
    E: Display + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        let _ = tx.send(fut.await.map_err(|e| e.to_string()));
    });
    cx.spawn(async move |this, cx| {
        if let Ok(Err(message)) = rx.await {
            tracing::warn!(error = %message, "optimistic persist failed");
            let _ = this.update(cx, |this, cx| {
                restore(this, snapshot, message, cx);
                cx.notify();
            });
        }
    })
    .detach();
}
