use forge_components::app_footer;
use gpui::{Context, Entity, Window, div, prelude::*};

use crate::presentation::{ActivePresentation, Presentation};
use crate::runtime_status::RuntimeStatus;

/// Application version, resolved from the crate at build time and inked by the kit
/// footer (base muted, prerelease stage tag in the brand accent).
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Connectable integration total the connection readout is measured against —
/// the five platform + stream-app entries in the sidebar. The connected count is
/// a stub `0` until the platform-health bridge topic lands.
const INTEGRATION_TOTAL: u8 = 5;
const CONNECTED_STUB: u8 = 0;

/// Persistent footer rendered as its own child view-entity. It composes
/// the kit `app_footer`, feeding it live uptime from the observed
/// [`RuntimeStatus`] topic entity plus the (stubbed) connection readout and the
/// build version. It holds only the topic handle — never runtime state — and
/// repaints when the bridge advances uptime.
pub struct Footer {
    status: Entity<RuntimeStatus>,
}

impl Footer {
    pub fn new(status: Entity<RuntimeStatus>, cx: &mut Context<Self>) -> Self {
        // Repaint on each uptime tick applied by the bridge, and on theme switch.
        cx.observe(&status, |_, _, cx| cx.notify()).detach();
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        Self { status }
    }
}

impl Render for Footer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let uptime = self.status.read(cx).uptime_human();

        let connected_label = format!("{CONNECTED_STUB}/{INTEGRATION_TOTAL} connected");
        let uptime_label = format!("{uptime} uptime");

        div().w_full().flex_none().child(app_footer(
            "forge",
            APP_VERSION,
            CONNECTED_STUB,
            connected_label,
            uptime_label,
            &palette,
        ))
    }
}
