use super::IntegrationDetail;
use crate::health_text::connection_label;
use crate::integration_catalog::{identity_of, tint_color};
use crate::integration_quick_actions::outline_icon_button;
use crate::user_text::user_text_string;
use forge_components::{
    ActivePresentation, Density, FONT_XS, ForgePalette, Icon, Radius, Spacing, badge,
    platform_hero, radius, scaled, spacing, status_dot, tr,
};
use forge_platform_core::{ConnectionState, HeaderAction, HeroBadge, HeroBadgeTone};
use forge_types::IntegrationId;
use gpui::{AnyElement, App, ClickEvent, Context, FontWeight, Pixels, Rgba, div, prelude::*, px};

const BADGE_PAD_X: Pixels = px(2.0);
const BADGE_PAD_Y: Pixels = px(6.0);
const STATUS_ROW_GAP: Pixels = px(8.0);
const STATUS_DOT_GAP: Pixels = px(5.0);
const STATUS_DOT: Pixels = px(7.0);

fn hero_identity(id: &IntegrationId, display_name: &str, palette: &ForgePalette) -> (String, Rgba) {
    if let Some(identity) = identity_of(id) {
        return (
            identity.monogram.to_owned(),
            tint_color(identity.hero_tint, palette),
        );
    }
    let initial = display_name
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "?".to_owned());
    (initial, palette.brand)
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
        .padding_xy(px(0.0), BADGE_PAD_Y)
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
    .padding_xy(BADGE_PAD_X, BADGE_PAD_Y)
    .radius(radius(Radius::Sm))
    .flex_none()
}

impl IntegrationDetail {
    pub(super) fn header_right_connected(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let mut row = div()
            .flex()
            .items_center()
            .gap(scaled(STATUS_ROW_GAP, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(scaled(STATUS_DOT_GAP, density))
                    .child(status_dot(palette.success, STATUS_DOT))
                    .child(
                        div()
                            .font_family(cx.body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.success)
                            .child(tr!("integration_status_authenticated")),
                    ),
            );
        if let Some(suffix) = self.shown.token_countdown.clone() {
            row = row
                .child(
                    div()
                        .text_size(FONT_XS)
                        .text_color(palette.text_faint)
                        .child("\u{00b7}"),
                )
                .child(
                    div()
                        .font_family(cx.mono_family())
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
        cx: &App,
    ) -> AnyElement {
        let (dot, text) = match self.shown.connection {
            ConnectionState::Connected => (palette.success, palette.success),
            ConnectionState::Connecting => (palette.info, palette.info),
            ConnectionState::Reconnecting => (palette.warning, palette.warning),
            ConnectionState::Disconnected => (palette.text_faint, palette.text_muted),
        };
        let label = connection_label(self.shown.connection);
        div()
            .flex()
            .items_center()
            .gap(scaled(STATUS_DOT_GAP, density))
            .child(status_dot(dot, STATUS_DOT))
            .child(
                div()
                    .font_family(cx.body_family())
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
        let (letter, brand) =
            hero_identity(self.ports.status.id(), &self.shown.display_name, palette);
        let hero_name = self
            .ports
            .status
            .hero_name()
            .unwrap_or_else(|| self.shown.display_name.clone());
        let sub = self.hero_subtitle();
        let name_badges = self.hero_badges(palette);

        let mut right = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density));
        if let Some(version) = &self.shown.version {
            right = right.child(pill(version.clone(), palette.text_muted, palette));
        }
        if self.shown.capability_flags.limited {
            let label = self
                .shown
                .capability_flags
                .label
                .as_ref()
                .map_or_else(|| tr!("widget_header_capability_limited"), user_text_string);
            right = right.child(pill(label.to_uppercase(), palette.warning, palette));
        }
        for (i, action) in self.shown.header_actions.iter().enumerate() {
            right = right.child(self.action_button(i, action.clone(), palette, cx));
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
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let signing_out = self.live.signing_out && matches!(action, HeaderAction::Disconnect);
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
        outline_icon_button(("header-action", idx), glyph, label, text_color, cx)
            .when(!signing_out, |button| {
                button
                    .cursor_pointer()
                    .hover(move |s| s.border_color(hover_border))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.on_header_action(action.clone(), window, cx)
                    }))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_hero_keeps_its_letter_and_tint() {
        let palette = forge_components::FORGE_DEFAULT;
        for (id, letter, tint) in [
            ("twitch", "T", palette.theme_brand),
            ("youtube", "Y", palette.danger),
            ("kick", "K", palette.info),
            ("donatello", "D", palette.accent_pink_light),
            ("monobank", "M", palette.text_secondary),
            ("obs", "O", palette.success),
            ("vtube", "V", palette.warning),
            ("discord", "D", palette.theme_brand),
            ("midi", "M", palette.accent_teal),
            ("hotkey", "H", palette.info),
        ] {
            assert_eq!(
                hero_identity(&IntegrationId::new(id), "ignored", &palette),
                (letter.to_owned(), tint),
                "{id}"
            );
        }
    }

    #[test]
    fn an_undeclared_hero_falls_back_to_the_display_initial() {
        let palette = forge_components::FORGE_DEFAULT;
        let unknown = IntegrationId::new("trovo");

        assert_eq!(
            hero_identity(&unknown, "trovo", &palette),
            ("T".to_owned(), palette.brand)
        );
        assert_eq!(
            hero_identity(&unknown, "", &palette),
            ("?".to_owned(), palette.brand)
        );
    }
}
