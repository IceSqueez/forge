use forge_components::{ForgePalette, app_footer, footer_alert, tr};
use gpui::{AnyElement, Context, Entity, SharedString, Window, div, prelude::*};

use crate::event_loss::{EventLoss, breakdown, consumer_label, tier_label};
use crate::platforms::PlatformConnectivity;
use crate::presentation::{ActivePresentation, Presentation};
use crate::runtime_status::RuntimeStatus;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct Footer {
    status: Entity<RuntimeStatus>,
    connectivity: Entity<PlatformConnectivity>,
    event_loss: Entity<EventLoss>,
}

impl Footer {
    pub fn new(
        status: Entity<RuntimeStatus>,
        connectivity: Entity<PlatformConnectivity>,
        event_loss: Entity<EventLoss>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&status, |_, _, cx| cx.notify()).detach();
        cx.observe(&connectivity, |_, _, cx| cx.notify()).detach();
        cx.observe(&event_loss, |_, _, cx| cx.notify()).detach();
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        Self {
            status,
            connectivity,
            event_loss,
        }
    }

    fn loss_badge(&self, palette: &ForgePalette, cx: &Context<Self>) -> Option<AnyElement> {
        let loss = self.event_loss.read(cx);
        let total = loss.total();
        if total == 0 {
            return None;
        }
        let color = if loss.priority_dropped() > 0 {
            palette.random
        } else {
            palette.text_secondary
        };
        let mut details: Vec<SharedString> = vec![tr!("event_loss_tooltip_title").into()];
        details.extend(loss.rows().iter().map(|row| {
            SharedString::from(tr!(
                "event_loss_tooltip_row",
                consumer = consumer_label(row.consumer),
                tier = tier_label(row.tier),
                detail = breakdown(&row.loss)
            ))
        }));
        let label = tr!(
            "event_loss_footer_badge",
            count = i64::try_from(total).unwrap_or(i64::MAX)
        );
        Some(footer_alert("footer-event-loss", label, color, details, palette).into_any_element())
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
        let loss_badge = self.loss_badge(&palette, cx);

        div().w_full().flex_none().child(app_footer(
            "forge",
            APP_VERSION,
            connected as u8,
            connected_label,
            uptime_label,
            loss_badge,
            &palette,
        ))
    }
}
