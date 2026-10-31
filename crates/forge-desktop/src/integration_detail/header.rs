use super::IntegrationDetail;
use crate::health_text::connection_label;
use crate::integration_catalog::{is_donation_service, look_of};
use crate::user_text::user_text_string;
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_XS, ForgePalette, Icon, Radius, Spacing, badge,
    body_family, icon, mono_family, platform_hero, radius, scaled, spacing, status_dot, tr,
};
use forge_platform_core::{ConnectionState, HeaderAction, HeroBadge, HeroBadgeTone};
use forge_types::IntegrationId;
use gpui::{AnyElement, ClickEvent, Context, FontWeight, Rgba, div, prelude::*, px};

fn hero_identity(
    id: &IntegrationId,
    icon_str: &str,
    display_name: &str,
    palette: &ForgePalette,
) -> (String, Rgba) {
    let look = look_of(id, palette);
    if let Some(letter) = look.letter.filter(|_| is_donation_service(id)) {
        return (letter.to_owned(), look.tint);
    }
    match icon_str {
        "brand-twitch" => ("T".to_owned(), palette.theme_brand),
        "brand-youtube" => ("Y".to_owned(), palette.danger),
        "brand-kick" => ("K".to_owned(), palette.info),
        "broadcast" => ("O".to_owned(), palette.success),
        "mood-smile" => ("V".to_owned(), palette.warning),
        "brand-discord" => ("D".to_owned(), palette.theme_brand),
        "piano" => ("M".to_owned(), palette.accent_teal),
        "keyboard" => ("H".to_owned(), palette.info),
        _ => {
            let initial = display_name
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase().to_string())
                .unwrap_or_else(|| "?".to_owned());
            (initial, palette.brand)
        }
    }
}

fn header_action_label(action: &HeaderAction) -> String {
    match action {
        HeaderAction::Reconnect => tr!("widget_header_action_reconnect"),
        HeaderAction::RefreshToken => tr!("widget_header_action_refresh_token"),
        HeaderAction::Disconnect => tr!("widget_header_action_disconnect"),
        HeaderAction::Settings => tr!("widget_header_action_settings"),
    }
}

fn pill(label: String, text_color: Rgba, palette: &ForgePalette) -> impl IntoElement {
    badge(palette.surface_overlay, text_color, label, true, FONT_XS)
        .weight(FontWeight::NORMAL)
        .padding_xy(px(0.0), px(6.0))
        .radius(radius(Radius::Md))
        .flex_none()
}

pub(super) fn hero_badge_elem(
    badge_spec: &HeroBadge,
    palette: &ForgePalette,
) -> impl IntoElement + use<> {
    let text_color = match badge_spec.tone {
        HeroBadgeTone::Neutral => palette.text_muted,
        HeroBadgeTone::Positive => palette.success,
    };
    badge(
        palette.surface_overlay,
        text_color,
        user_text_string(&badge_spec.label),
        badge_spec.monospace,
        FONT_XS,
    )
    .weight(FontWeight::NORMAL)
    .padding_xy(px(2.0), px(6.0))
    .radius(radius(Radius::Sm))
    .flex_none()
}

impl IntegrationDetail {
    pub(super) fn header_right_connected(
        &self,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let mut row = div()
            .flex()
            .items_center()
            .gap(scaled(px(8.0), density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(scaled(px(5.0), density))
                    .child(status_dot(palette.success, px(7.0)))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.success)
                            .child(tr!("integration_status_authenticated")),
                    ),
            );
        if let Some(suffix) = self.token_countdown() {
            row = row
                .child(
                    div()
                        .text_size(FONT_XS)
                        .text_color(palette.text_faint)
                        .child("\u{00b7}"),
                )
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(suffix),
                );
        }
        row.into_any_element()
    }

    pub(super) fn header_right_connection(
        &self,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let (dot, text) = match self.connection {
            ConnectionState::Connected => (palette.success, palette.success),
            ConnectionState::Connecting => (palette.info, palette.info),
            ConnectionState::Reconnecting => (palette.warning, palette.warning),
            ConnectionState::Disconnected => (palette.text_faint, palette.text_muted),
        };
        let label = connection_label(self.connection);
        div()
            .flex()
            .items_center()
            .gap(scaled(px(5.0), density))
            .child(status_dot(dot, px(7.0)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(text)
                    .child(label),
            )
            .into_any_element()
    }

    pub(super) fn header_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (letter, brand) = hero_identity(
            self.status.id(),
            self.icon.as_str(),
            &self.display_name,
            palette,
        );
        let hero_name = self
            .status
            .hero_name()
            .unwrap_or_else(|| self.display_name.clone());
        let sub = self.hero_subtitle();
        let name_badges = self.hero_badges(palette);

        let mut right = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density));
        if let Some(version) = &self.version {
            right = right.child(pill(version.clone(), palette.text_muted, palette));
        }
        if self.capability_flags.limited {
            let label = self
                .capability_flags
                .label
                .as_ref()
                .map_or_else(|| tr!("widget_header_capability_limited"), user_text_string);
            right = right.child(pill(label.to_uppercase(), palette.warning, palette));
        }
        for (i, action) in self.header_actions.iter().enumerate() {
            right = right.child(self.action_button(i, action.clone(), palette, density, cx));
        }

        platform_hero(letter, brand, hero_name, sub, palette)
            .density(density)
            .name_badges(name_badges)
            .right(right)
            .into_any_element()
    }

    fn action_button(
        &self,
        idx: usize,
        action: HeaderAction,
        palette: &ForgePalette,
        _density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let signing_out = self.signing_out && matches!(action, HeaderAction::Disconnect);
        let label = if signing_out {
            tr!("integration_signing_out")
        } else {
            header_action_label(&action)
        };
        let text_color = match action {
            _ if signing_out => palette.text_muted,
            HeaderAction::Disconnect => palette.danger,
            _ => palette.text_secondary,
        };
        let glyph = match action {
            HeaderAction::Reconnect | HeaderAction::RefreshToken => Icon::Refresh,
            HeaderAction::Disconnect => Icon::Logout,
            HeaderAction::Settings => Icon::Settings,
        };
        let hover_border = palette.border_input;
        div()
            .id(("header-action", idx))
            .flex()
            .items_center()
            .gap(scaled(px(5.0), density))
            .py(scaled(px(5.0), density))
            .px(scaled(px(11.0), density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .when(!signing_out, |button| {
                button
                    .cursor_pointer()
                    .hover(move |s| s.border_color(hover_border))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.on_header_action(action.clone(), window, cx)
                    }))
            })
            .child(icon(glyph, FONT_XS, text_color))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(text_color)
                    .child(label),
            )
            .into_any_element()
    }
}
