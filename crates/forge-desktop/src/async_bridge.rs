use std::fmt::Display;
use std::future::Future;
use std::path::PathBuf;

use forge_components::ToastKind;
use forge_events::{Event, EventsError};
use forge_runtime::{EventBus, EventSubscription};
use gpui::{App, AsyncApp, Context, SharedString};
use tokio::runtime::Handle;

use crate::toasts::PushToast;

const BRIDGE_DRAIN_CAP: usize = 128;

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
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        let _ = tx.send(dialog.await);
    });
    cx.spawn(async move |this, cx| {
        if let Ok(result) = rx.await {
            let _ = this.update(cx, |this, cx| on_result(this, result, cx));
        }
    })
    .detach();
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
