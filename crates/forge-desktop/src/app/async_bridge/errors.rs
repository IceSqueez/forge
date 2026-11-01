use std::fmt::Display;
use std::future::Future;

use forge_components::{PushToast, ToastKind};
use gpui::{App, Context, SharedString};
use tokio::runtime::Handle;

use super::tasks::on_runtime;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorSink {
    Toast,
    Silent,
}

impl ErrorSink {
    pub(crate) fn report(self, message: String, cx: &mut App) {
        if self == ErrorSink::Toast {
            cx.push_toast(ToastKind::Error, message);
        }
    }
}

pub(crate) fn report_failure<V, F, E>(
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
    let reply = on_runtime(handle, fut);
    cx.spawn(async move |_this, cx| {
        if let Ok(Err(error)) = reply.await {
            tracing::warn!(error = %error, context = %context, "async operation failed");
            cx.update(|cx| {
                sink.report(context.to_string(), cx);
            });
        }
    })
    .detach();
}

pub(crate) fn detached<F, E>(handle: &Handle, context: &'static str, fut: F)
where
    F: Future<Output = Result<(), E>> + Send + 'static,
    E: Display + Send + 'static,
{
    handle.spawn(async move {
        if let Err(e) = fut.await {
            tracing::warn!(error = %e, context = %context, "detached write failed");
        }
    });
}
