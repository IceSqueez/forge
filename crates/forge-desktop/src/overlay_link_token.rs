use forge_server::ServerHandle;
use forge_server::masked_overlay_token;
use gpui::{App, Context, Global, Subscription};

struct OverlayLinkToken(Option<String>);

impl Global for OverlayLinkToken {}

pub(crate) fn current(cx: &App) -> Option<String> {
    cx.try_global::<OverlayLinkToken>()
        .and_then(|token| token.0.clone())
}

pub(crate) fn publish(token: String, cx: &mut App) {
    cx.set_global(OverlayLinkToken(Some(token)));
}

pub(crate) fn follow<V: 'static>(
    server: Option<&ServerHandle>,
    rt_handle: &tokio::runtime::Handle,
    apply: fn(&mut V, Option<String>),
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
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt_handle.spawn(async move {
        let _ = tx.send(handle.auth_state().await.overlay_token().current());
    });
    cx.spawn(async move |cx| {
        if let Ok(token) = rx.await {
            cx.update(|cx| publish(token, cx));
        }
    })
    .detach();
}
