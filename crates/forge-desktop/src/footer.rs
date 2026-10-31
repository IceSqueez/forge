use forge_components::{ForgePalette, Icon, app_footer, footer_alert, footer_indicator, tr};
use gpui::{AnyElement, Context, Entity, SharedString, Window, div, prelude::*};

use crate::awake_state::{AwakeState, AwakeTone};
use crate::event_loss::{EventLoss, breakdown, consumer_label, tier_label};
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::platforms::PlatformConnectivity;
use crate::runtime_status::RuntimeStatus;
use forge_components::{ActivePresentation, Presentation};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) struct Footer {
    status: Entity<RuntimeStatus>,
    connectivity: Entity<PlatformConnectivity>,
    event_loss: Entity<EventLoss>,
    awake: Entity<AwakeState>,
    lifecycle: Entity<IntegrationLifecycle>,
}

impl Footer {
    pub(crate) fn new(
        status: Entity<RuntimeStatus>,
        connectivity: Entity<PlatformConnectivity>,
        event_loss: Entity<EventLoss>,
        awake: Entity<AwakeState>,
        lifecycle: Entity<IntegrationLifecycle>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&status, |_, _, cx| cx.notify()).detach();
        cx.observe(&connectivity, |_, _, cx| cx.notify()).detach();
        cx.observe(&event_loss, |_, _, cx| cx.notify()).detach();
        cx.observe(&awake, |_, _, cx| cx.notify()).detach();
        cx.observe(&lifecycle, |_, _, cx| cx.notify()).detach();
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        Self {
            status,
            connectivity,
            event_loss,
            awake,
            lifecycle,
        }
    }

    fn awake_badge(&self, palette: &ForgePalette, cx: &Context<Self>) -> Option<AnyElement> {
        let density = cx.density();
        let indicator = self.awake.read(cx).indicator()?;
        let (icon_color, label_color) = match indicator.tone {
            AwakeTone::Normal => (palette.text_faint, palette.text_secondary),
            AwakeTone::Warning => (palette.warning, palette.warning),
        };
        let details = indicator
            .details
            .into_iter()
            .map(SharedString::from)
            .collect();
        Some(
            footer_indicator(
                "footer-stay-awake",
                Icon::Coffee,
                tr!("stay_awake_footer_label"),
                icon_color,
                label_color,
                details,
                palette,
                density,
            )
            .into_any_element(),
        )
    }

    fn loss_badge(&self, palette: &ForgePalette, cx: &Context<Self>) -> Option<AnyElement> {
        let density = cx.density();
        let loss = self.event_loss.read(cx);
        let total = loss.total();
        if total == 0 {
            return None;
        }
        let color = if loss.priority_dropped() > 0 {
            palette.danger
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
        Some(
            footer_alert("footer-event-loss", label, color, details, palette, density)
                .into_any_element(),
        )
    }
}

impl Render for Footer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();
        let uptime = self.status.read(cx).uptime_human();

        let lifecycle = self.lifecycle.read(cx);
        let (connected, total) = self
            .connectivity
            .read(cx)
            .tally_enabled(|integ| lifecycle.is_on(&integ.builtin_id()));
        let connected_label = tr!(
            "footer_connected",
            connected = i64::try_from(connected).unwrap_or(i64::MAX),
            total = i64::try_from(total).unwrap_or(i64::MAX)
        );
        let uptime_label = tr!("footer_uptime", uptime = uptime.as_str());
        let trailing: Vec<AnyElement> = [
            self.awake_badge(&palette, cx),
            self.loss_badge(&palette, cx),
        ]
        .into_iter()
        .flatten()
        .collect();

        div().w_full().flex_none().child(app_footer(
            "forge",
            APP_VERSION,
            connected as u8,
            connected_label,
            uptime_label,
            trailing,
            &palette,
            density,
        ))
    }
}
