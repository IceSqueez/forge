use forge_components::{DEFAULT_BODY_FAMILY, Density, FONT_XS, Icon, Spacing, icon, spacing};
use gpui::{Context, Entity, Subscription, Window, div, prelude::*, px};

use crate::presentation::ActivePresentation;
use crate::runtime_status::RuntimeStatus;

pub struct UptimeView {
    status: Entity<RuntimeStatus>,
    _status_obs: Subscription,
}

impl UptimeView {
    pub fn new(status: Entity<RuntimeStatus>, cx: &mut Context<Self>) -> Self {
        let status_obs = cx.observe(&status, |_, _, cx| cx.notify());
        Self {
            status,
            _status_obs: status_obs,
        }
    }
}

impl Render for UptimeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let uptime_text = self.status.read(cx).uptime_human();

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(icon(Icon::Clock, px(12.0), palette.text_muted))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(uptime_text),
            )
    }
}
