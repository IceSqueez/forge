use forge_storage::OverlayId;
use gpui::Context;

use super::OverlaysView;
use crate::overlay_url::{overlay_origin, overlay_page_url, resolve_routable_host};

struct ServedEndpoint {
    bind_address: String,
    routable_host: Option<String>,
}

#[derive(Default)]
pub(super) struct ServerStatus {
    pub(super) running: bool,
    pub(super) bind_address: Option<String>,
    routable_host: Option<String>,
}

impl ServerStatus {
    pub(super) fn new(running: bool) -> Self {
        Self {
            running,
            ..Self::default()
        }
    }

    fn overlay_url(&self, id: &OverlayId) -> Option<String> {
        if !self.running {
            return None;
        }
        let origin = overlay_origin(self.bind_address.as_deref()?, self.routable_host.as_deref());
        Some(overlay_page_url(&origin, id.as_str()))
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

    pub(super) fn start_server_bridge(&self, cx: &mut Context<Self>) {
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
