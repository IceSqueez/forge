use std::fmt::Display;
use std::future::Future;

use forge_components::ToastKind;
use gpui::{App, Context, SharedString};
use tokio::runtime::Handle;

use crate::toasts::PushToast;

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
