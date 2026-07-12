use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, FONT_LG, FONT_SM, FONT_XS, Radius, Spacing, icon, radius,
    spacing,
};
use gpui::{Context, Window, div, prelude::*};

use crate::presentation::ActivePresentation;
use crate::screen::Screen;

/// Placeholder content view-entity standing in for every routed screen until the
/// real per-screen views land. One concrete type serves the whole seed roster;
/// as real screens arrive each becomes its own view-entity and the router's
/// active-content field widens to an erased view.
pub struct ScreenStub {
    screen: Screen,
}

impl ScreenStub {
    pub fn new(screen: Screen) -> Self {
        Self { screen }
    }
}

impl Render for ScreenStub {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(icon(self.screen.icon(), FONT_LG, palette.brand))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_LG)
                    .text_color(palette.text_primary)
                    .child(self.screen.title()),
            );

        let placeholder = div()
            .w_full()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .rounded(radius(Radius::Md))
            .bg(palette.elevated)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_faint)
                    .child("—"),
            );

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .p(spacing(Spacing::Lg, density))
            .bg(palette.base)
            .child(header)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child("Screen content lands in a later migration slice."),
            )
            .child(placeholder)
    }
}
