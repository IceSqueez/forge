use gpui::{IntoElement, ParentElement, SharedString, Styled, div};

use crate::palette::ForgePalette;
use crate::tokens::{DEFAULT_MONO_FAMILY, FONT_XXS};

pub fn section_label(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label.into())
}
