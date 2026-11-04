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

#[cfg(test)]
mod tests {
    use gpui::{
        Bounds, Context, InteractiveElement, Render, Styled, TestAppContext, VisualTestContext,
    };

    use super::*;
    use crate::buttons::icon_button;
    use crate::icons::Icon;
    use crate::palette::FORGE_DEFAULT;
    use crate::test_support::install_presentation;

    const PANE: &str = "selection-bar-test-pane";
    const ACTION: &str = "selection-bar-test-action";
    const CLEAR: &str = "selection-bar-test-clear";
    const PANE_W: f32 = 140.0;
    const PANE_H: f32 = 240.0;
    const LONG_COUNT: &str = "12345 selected rows across every group in the list";

    struct Pane;

    impl Render for Pane {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let bar = selection_bar(LONG_COUNT, &FORGE_DEFAULT)
                .density(Density::Cozy)
                .action(icon_button(Icon::Eraser, &FORGE_DEFAULT).on_click(ACTION, |_, _, _| {}))
                .clear(icon_button(Icon::X, &FORGE_DEFAULT).on_click(CLEAR, |_, _, _| {}));
            div()
                .debug_selector(|| PANE.to_owned())
                .relative()
                .w(px(PANE_W))
                .h(px(PANE_H))
                .child(bar)
        }
    }

    fn painted(vcx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
        vcx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("nothing painted {selector}"))
    }

    #[gpui::test]
    fn a_long_count_keeps_every_button_on_one_line_inside_the_bar_docked_at_the_pane_bottom(
        cx: &mut TestAppContext,
    ) {
        install_presentation(cx);
        let (_pane, vcx) = cx.add_window_view(|_, _| Pane);
        vcx.run_until_parked();

        let pane = painted(vcx, PANE);
        let bar_top = pane.bottom() - selection_bar_height(Density::Cozy);
        for selector in [ACTION, CLEAR] {
            let button = painted(vcx, selector);
            assert!(
                button.left() >= pane.left()
                    && button.right() <= pane.right()
                    && button.top() >= bar_top
                    && button.bottom() <= pane.bottom(),
                "{selector} at {button:?} outside the bar of {pane:?}"
            );
        }
    }
}
