use super::IntegrationDetail;
use crate::user_text::resolve_message;
use forge_components::{
    BORDER_THIN, CTA_HOVER_ALPHA, ConfirmTone, Density, FONT_SM, FONT_XS, ForgePalette, Icon,
    OverlayPosition, Radius, Spacing, body_family, confirm_modal, icon, overlay, radius, spacing,
    tr, with_alpha,
};
use forge_platform_core::ConnectionState;
use forge_types::PlatformId;
use gpui::{AnyElement, ClickEvent, Context, Rgba, div, prelude::*, px};

impl IntegrationDetail {
    pub(super) fn disconnect_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let card = confirm_modal(
            tr!("integration_disconnect_title"),
            tr!("builtin_disconnect_confirm_hint"),
            ConfirmTone::Warning,
            palette,
        )
        .item_name(self.display_name.clone())
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "integration-disconnect-cancel",
            tr!("widget_confirm_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disconnect(cx)),
        )
        .on_confirm(
            "integration-disconnect-confirm",
            tr!("widget_header_action_disconnect"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_disconnect(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("integration-disconnect-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_disconnect(cx));
            })
    }

    pub(super) fn twitch_reauth_banner(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cta = div()
            .id("twitch-reauth")
            .flex_none()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.warning)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.warning, CTA_HOVER_ALPHA)))
            .on_click(cx.listener(|this, _, _, cx| this.reset_to_connect(PlatformId::Twitch, cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(tr!("twitch_reauth_btn")),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.warning)
            .bg(palette.shell)
            .child(icon(Icon::AlertTriangle, px(14.0), palette.warning))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("twitch_reauth_title")),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("twitch_reauth_detail")),
                    ),
            )
            .child(cta)
            .into_any_element()
    }

    pub(super) fn state_banner(
        &self,
        palette: &ForgePalette,
        density: Density,
    ) -> Option<AnyElement> {
        let (accent, glyph, title, detail): (Rgba, Icon, String, String) = match self.connection {
            ConnectionState::Connected => return None,
            ConnectionState::Disconnected if self.donation_settings.is_some() => return None,
            ConnectionState::Connecting => (
                palette.info,
                Icon::Refresh,
                tr!("integration_state_connecting_title"),
                tr!("integration_state_connecting_detail"),
            ),
            ConnectionState::Reconnecting => (
                palette.warning,
                Icon::Refresh,
                tr!("integration_state_reconnecting_title"),
                tr!("integration_state_reconnecting_detail"),
            ),
            ConnectionState::Disconnected if self.obs_password_rejected => (
                palette.warning,
                Icon::AlertTriangle,
                tr!("obs_state_password_rejected_title"),
                tr!("obs_state_password_rejected_detail"),
            ),
            ConnectionState::Disconnected => (
                palette.text_muted,
                Icon::PlugConnected,
                tr!("common_status_not_connected"),
                tr!("integration_state_disconnected_detail"),
            ),
        };

        let failure_reason = if self.obs_password_rejected {
            None
        } else {
            self.status
                .connect_failure()
                .map(|failure| resolve_message(&failure).inline())
        };

        let text_col = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(title),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(detail),
            )
            .children(failure_reason.map(|reason| {
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(reason)
            }));

        Some(
            div()
                .w_full()
                .flex()
                .items_start()
                .gap(spacing(Spacing::Sm, density))
                .py(spacing(Spacing::Sm, density))
                .px(spacing(Spacing::Md, density))
                .rounded(radius(Radius::Md))
                .border(BORDER_THIN)
                .border_color(accent)
                .bg(palette.elevated)
                .child(icon(glyph, FONT_SM, accent))
                .child(text_col)
                .into_any_element(),
        )
    }
}
