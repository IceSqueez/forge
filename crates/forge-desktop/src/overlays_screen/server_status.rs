use forge_storage::OverlayId;
use gpui::{Context, Subscription};

use super::OverlaysView;
use crate::overlay_link_token;
use forge_server::overlay_url::{overlay_origin, overlay_page_url, resolve_routable_host};

struct ServedEndpoint {
    bind_address: String,
    routable_host: Option<String>,
}

#[derive(Default)]
pub(super) struct ServerStatus {
    pub(super) running: bool,
    pub(super) bind_address: Option<String>,
    routable_host: Option<String>,
    token: Option<String>,
    token_watch: Option<Subscription>,
}

impl ServerStatus {
    pub(super) fn new(running: bool) -> Self {
        Self {
            running,
            ..Self::default()
        }
    }

    fn overlay_url(&self, id: &OverlayId) -> Option<String> {
        self.page_url(self.token.as_deref()?, id)
    }

    fn displayed_overlay_url(&self, id: &OverlayId) -> Option<String> {
        self.page_url(&overlay_link_token::displayed(self.token.as_deref()?), id)
    }

    fn page_url(&self, token: &str, id: &OverlayId) -> Option<String> {
        if !self.running {
            return None;
        }
        let origin = overlay_origin(self.bind_address.as_deref()?, self.routable_host.as_deref());
        Some(overlay_page_url(&origin, token, id.as_str()))
    }

    fn apply(&mut self, running: bool, endpoint: Option<ServedEndpoint>) -> bool {
        let mut changed = self.running != running;
        self.running = running;
        if let Some(endpoint) = endpoint
            && (self.bind_address.as_deref() != Some(endpoint.bind_address.as_str())
                || self.routable_host != endpoint.routable_host)
        {
            self.bind_address = Some(endpoint.bind_address);
            self.routable_host = endpoint.routable_host;
            changed = true;
        }
        changed
    }
}

impl OverlaysView {
    pub(super) fn overlay_url(&self, id: &OverlayId) -> Option<String> {
        self.served.overlay_url(id)
    }

    pub(super) fn displayed_overlay_url(&self, id: &OverlayId) -> Option<String> {
        self.served.displayed_overlay_url(id)
    }

    pub(super) fn displayed_served_path(&self, id: &OverlayId, file: &str) -> String {
        let token = overlay_link_token::displayed(self.served.token.as_deref().unwrap_or_default());
        format!("{}{file}", overlay_page_url("", &token, id.as_str()))
    }

    fn follow_overlay_token(&mut self, cx: &mut Context<Self>) {
        self.served.token = overlay_link_token::current(cx);
        self.served.token_watch = Some(overlay_link_token::follow(
            self.handles.server.as_ref(),
            &self.handles.rt_handle,
            |this: &mut Self, token| this.served.token = token,
            cx,
        ));
    }

    pub(super) fn start_server_bridge(&mut self, cx: &mut Context<Self>) {
        self.follow_overlay_token(cx);
        let Some(handle) = self.handles.server.clone() else {
            return;
        };
        let rt_handle = self.handles.rt_handle.clone();
        let mut run_state = handle.run_state();
        cx.spawn(async move |this, cx| {
            loop {
                let running = *run_state.borrow_and_update();
                let (tx, rx) = tokio::sync::oneshot::channel();
                let probe = handle.clone();
                rt_handle.spawn(async move {
                    let bind_address = probe.bind_addr().await.to_string();
                    let routable_host = resolve_routable_host(&bind_address);
                    let _ = tx.send(ServedEndpoint {
                        bind_address,
                        routable_host,
                    });
                });
                let endpoint = rx.await.ok();
                if this
                    .update(cx, |this, cx| {
                        this.apply_server_state(running, endpoint, cx)
                    })
                    .is_err()
                    || run_state.changed().await.is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply_server_state(
        &mut self,
        running: bool,
        endpoint: Option<ServedEndpoint>,
        cx: &mut Context<Self>,
    ) {
        if self.served.apply(running, endpoint) {
            cx.notify();
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    const TOKEN: &str = "fgo_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_abcde";

    fn serving(token: Option<&str>) -> ServerStatus {
        ServerStatus {
            running: true,
            bind_address: Some("127.0.0.1:9515".to_owned()),
            token: token.map(str::to_owned),
            ..ServerStatus::default()
        }
    }

    #[test]
    fn the_copied_overlay_url_carries_the_full_token_ahead_of_the_identity() {
        let url = serving(Some(TOKEN)).overlay_url(&OverlayId::new("alerts"));

        assert_eq!(
            url.as_deref(),
            Some(format!("http://127.0.0.1:9515/overlays/{TOKEN}/alerts/").as_str())
        );
    }

    #[test]
    fn the_displayed_overlay_url_never_shows_the_full_token() {
        let shown = serving(Some(TOKEN))
            .displayed_overlay_url(&OverlayId::new("alerts"))
            .expect("a running server offers a URL");

        assert!(!shown.contains(TOKEN), "shown {shown}");
        assert!(shown.ends_with("/alerts/"), "shown {shown}");
    }

    #[test]
    fn no_overlay_url_is_offered_before_the_token_is_known() {
        let status = serving(None);
        let id = OverlayId::new("alerts");

        assert_eq!(status.overlay_url(&id), None);
        assert_eq!(status.displayed_overlay_url(&id), None);
    }
}
