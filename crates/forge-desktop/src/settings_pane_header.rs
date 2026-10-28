use forge_components::FONT_ROW;
use forge_components::{ActivePresentation, FONT_LG, Spacing, body_family, icon, scaled, spacing};
use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, Pixels, RenderOnce, SharedString,
    Styled, Window, div, px,
};

use crate::navigation::SettingsSection;

const ICON_SIZE: Pixels = px(20.0);
const SUBTITLE_FONT: Pixels = FONT_ROW;
const CONTENT_GAP_AFTER_SUBTITLE: Pixels = px(22.0);
const CONTENT_GAP_AFTER_TITLE: Pixels = px(18.0);

#[derive(IntoElement)]
pub(crate) struct SettingsPaneHeader {
    section: SettingsSection,
    subtitle: Option<SharedString>,
    trailing: Vec<AnyElement>,
}

impl SettingsPaneHeader {
    pub(crate) fn new(section: SettingsSection) -> Self {
        Self {
            section,
            subtitle: None,
            trailing: Vec::new(),
        }
    }

    pub(crate) fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub(crate) fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing.push(element.into_any_element());
        self
    }
}

impl RenderOnce for SettingsPaneHeader {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let mut title_row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(icon(self.section.icon(), ICON_SIZE, palette.brand))
            .child(
                div()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_LG)
                    .text_color(palette.text_primary)
                    .child(self.section.label()),
            );
        if !self.trailing.is_empty() {
            title_row = title_row.child(div().flex_1().min_w(px(0.0))).child(
                div()
                    .flex()
                    .items_center()
                    .min_w(px(0.0))
                    .gap(spacing(Spacing::Xs, density))
                    .children(self.trailing),
            );
        }

        let header = div().w_full().flex().flex_col().child(title_row);
        match self.subtitle {
            Some(subtitle) => header
                .gap(spacing(Spacing::Xs, density))
                .mb(scaled(CONTENT_GAP_AFTER_SUBTITLE, density))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(SUBTITLE_FONT)
                        .text_color(palette.text_muted)
                        .child(subtitle),
                ),
            None => header.mb(scaled(CONTENT_GAP_AFTER_TITLE, density)),
        }
    }
}
