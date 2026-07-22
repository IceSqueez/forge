use forge_components::app_footer;
use gpui::{Context, Entity, Window, div, prelude::*};

use crate::platforms::PlatformConnectivity;
use crate::presentation::{ActivePresentation, Presentation};
use crate::runtime_status::RuntimeStatus;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct Footer {
    status: Entity<RuntimeStatus>,
    connectivity: Entity<PlatformConnectivity>,
}

impl Footer {
    pub fn new(
        status: Entity<RuntimeStatus>,
        connectivity: Entity<PlatformConnectivity>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&status, |_, _, cx| cx.notify()).detach();
        cx.observe(&connectivity, |_, _, cx| cx.notify()).detach();
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        Self {
            status,
            connectivity,
        }
    }
}

impl Render for Footer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let uptime = self.status.read(cx).uptime_human();

        let connectivity = self.connectivity.read(cx);
        let connected = connectivity.connected_count();
        let total = connectivity.total_count();
        let connected_label = format!("{connected}/{total} connected");
        let uptime_label = format!("{uptime} uptime");

        div().w_full().flex_none().child(app_footer(
            "forge",
            APP_VERSION,
            connected as u8,
            connected_label,
            uptime_label,
            &palette,
        ))
    }
}
