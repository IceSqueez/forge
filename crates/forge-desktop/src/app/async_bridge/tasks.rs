use std::future::Future;

use gpui::{App, Context, Entity, Window};
use tokio::runtime::Handle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the runtime stopped before the work finished")]
pub(crate) struct RuntimeGone;

pub(crate) fn on_runtime<Fut>(
    handle: &Handle,
    fut: Fut,
) -> impl Future<Output = Result<Fut::Output, RuntimeGone>> + use<Fut>
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    let task = handle.spawn(fut);
    async move { task.await.map_err(|_| RuntimeGone) }
}

pub(crate) fn on_runtime_blocking<Work, T>(
    handle: &Handle,
    work: Work,
) -> impl Future<Output = Result<T, RuntimeGone>> + use<Work, T>
where
    Work: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let task = handle.spawn_blocking(work);
    async move { task.await.map_err(|_| RuntimeGone) }
}

pub(crate) fn run_async<V, Fut, T>(
    handle: &Handle,
    fut: Fut,
    apply: impl FnOnce(&mut V, T, &mut Context<V>) + Send + 'static,
    cx: &mut Context<V>,
) where
    V: 'static,
    T: Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let reply = on_runtime(handle, fut);
    cx.spawn(async move |this, cx| {
        if let Ok(result) = reply.await {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| apply(this, result, cx));
        }
    })
    .detach();
}

pub(crate) fn attach_to_window<V: 'static>(
    attach: impl FnOnce(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) {
    let Some(window) = cx.windows().first().copied() else {
        return;
    };
    cx.spawn(async move |this, cx| {
        let attached = window.update(cx, |_, window, cx| {
            this.update(cx, |this, cx| attach(this, window, cx))
        });
        if !matches!(attached, Ok(Ok(()))) {
            tracing::debug!("the view or its window closed before it could attach");
        }
    })
    .detach();
}

pub(crate) fn run_async_entity<V, Fut, T>(
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
    let reply = on_runtime(handle, fut);
    app.spawn(async move |cx| {
        if let Ok(result) = reply.await {
            view.update(cx, |this, cx| apply(this, result, cx));
        }
    })
    .detach();
}

pub(crate) fn run_blocking<V, Work, T>(
    handle: &Handle,
    work: Work,
    apply: impl FnOnce(&mut V, T, &mut Context<V>) + Send + 'static,
    cx: &mut Context<V>,
) where
    V: 'static,
    T: Send + 'static,
    Work: FnOnce() -> T + Send + 'static,
{
    let reply = on_runtime_blocking(handle, work);
    cx.spawn(async move |this, cx| {
        if let Ok(result) = reply.await {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| apply(this, result, cx));
        }
    })
    .detach();
}
