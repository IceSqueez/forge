use gpui::{
    App, IntoElement, ParentElement, Pixels, RenderOnce, SharedString, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::buttons::Button;
use crate::palette::ForgePalette;
use crate::presentation::ActivePresentation;
use crate::tokens::{BORDER_THIN, Density, FONT_XS, Spacing, scaled, spacing};

const SELECTION_BAR_HEIGHT: Pixels = px(36.0);
const DIVIDER_HEIGHT: Pixels = px(16.0);

pub fn selection_bar_height(density: Density) -> Pixels {
    scaled(SELECTION_BAR_HEIGHT, density)
}

#[derive(IntoElement)]
pub struct SelectionBar {
    count_label: SharedString,
    palette: ForgePalette,
    density: Option<Density>,
    actions: Vec<Button>,
    clear: Option<Button>,
}

pub fn selection_bar(count_label: impl Into<SharedString>, palette: &ForgePalette) -> SelectionBar {
    SelectionBar {
        count_label: count_label.into(),
        palette: *palette,
        density: None,
        actions: Vec::new(),
        clear: None,
    }
}

impl SelectionBar {
    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = Some(density);
        self
    }

    #[must_use]
    pub fn action(mut self, button: Button) -> Self {
        self.actions.push(button);
        self
    }

    #[must_use]
    pub fn clear(mut self, button: Button) -> Self {
        self.clear = Some(button);
        self
    }
}

impl RenderOnce for SelectionBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let density = self.density.unwrap_or_else(|| cx.density());
        let palette = self.palette;
        let actions = self
            .actions
            .into_iter()
            .map(|button| button.density(density));
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(selection_bar_height(density))
            .flex()
            .flex_nowrap()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .bg(palette.elevated)
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(self.count_label),
            )
            .children(actions)
            .when_some(self.clear, |bar, clear| {
                bar.child(
                    div()
                        .flex_none()
                        .w(BORDER_THIN)
                        .h(scaled(DIVIDER_HEIGHT, density))
                        .mx(spacing(Spacing::Xxs, density))
                        .bg(palette.border_regular),
                )
                .child(clear.density(density))
            })
    }
}
