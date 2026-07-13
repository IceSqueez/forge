use forge_components::app_footer;
use gpui::{Context, Entity, Window, div, prelude::*};

use crate::platforms::PlatformConnectivity;
use crate::presentation::{ActivePresentation, Presentation};
use crate::runtime_status::RuntimeStatus;

/// Application version, resolved from the crate at build time and inked by the kit
/// footer (base muted, prerelease stage tag in the brand accent).
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Persistent footer rendered as its own child view-entity. It composes the kit
/// `app_footer`, feeding it live uptime from the observed [`RuntimeStatus`] topic
/// entity plus the connected/total readout from the observed [`PlatformConnectivity`]
/// topic and the build version. It holds only the topic handles — never runtime
/// state — and repaints when the bridge advances uptime or a connection changes.
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
        // Repaint on each uptime tick applied by the bridge, on a connection change,
        // and on theme switch.
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
