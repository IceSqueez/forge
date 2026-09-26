use gpui::{IntoElement, ParentElement, SharedString, Styled, div};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, Density, FONT_XXS, Spacing, mono_family, spacing};

pub fn chat_gap_row(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
    density: Density,
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
                        .font_family(mono_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_secondary)
                        .child(label.into()),
                ),
        )
        .child(rule())
}
