use gpui::{App, IntoElement, ParentElement, SharedString, Styled, div};

use forge_components::ForgePalette;
use forge_components::{ActivePresentation, BORDER_THIN, Density, FONT_XXS, Spacing, spacing};
use forge_components::{Icon, icon};

pub(crate) fn chat_gap_row(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> impl IntoElement {
    let rule = || div().flex_1().h(BORDER_THIN).bg(palette.border_regular);
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Xxs, density))
        .child(rule())
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .child(icon(Icon::AlertTriangle, FONT_XXS, palette.text_muted))
                .child(
                    div()
                        .font_family(cx.mono_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_secondary)
                        .child(label.into()),
                ),
        )
        .child(rule())
}
