use forge_server::ServerHandle;
use forge_server::masked_overlay_token;
use forge_types::Secret;
use gpui::{App, Context, Global, Subscription};

struct OverlayLinkToken(Option<Secret>);

impl Global for OverlayLinkToken {}

pub(crate) fn current(cx: &App) -> Option<Secret> {
    cx.try_global::<OverlayLinkToken>()
        .and_then(|token| token.0.clone())
}

pub(crate) fn publish(token: Secret, cx: &mut App) {
    cx.set_global(OverlayLinkToken(Some(token)));
}

pub(crate) fn follow<V: 'static>(
    server: Option<&ServerHandle>,
    rt_handle: &tokio::runtime::Handle,
    apply: fn(&mut V, Option<Secret>),
    cx: &mut Context<V>,
) -> Subscription {
    load_once(server, rt_handle, cx);
    cx.observe_global::<OverlayLinkToken>(move |this, cx| {
        apply(this, current(cx));
        cx.notify();
    })
}

pub(crate) fn displayed(token: &str) -> String {
    masked_overlay_token(token)
}

fn load_once(server: Option<&ServerHandle>, rt_handle: &tokio::runtime::Handle, cx: &mut App) {
    if cx.has_global::<OverlayLinkToken>() {
        return;
    }
    let Some(handle) = server.cloned() else {
        return;
    };
    cx.set_global(OverlayLinkToken(None));
    let reply = crate::async_bridge::on_runtime(rt_handle, async move { handle.overlay_token() });
    cx.spawn(async move |cx| match reply.await {
        Ok(token) => cx.update(|cx| publish(token, cx)),
        Err(error) => tracing::warn!(%error, "overlay link token lookup stopped"),
    })
    .detach();
}
