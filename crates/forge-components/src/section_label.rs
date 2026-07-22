use gpui::{IntoElement, ParentElement, SharedString, Styled, div};

use crate::palette::ForgePalette;
use crate::tokens::{FONT_XXS, mono_family};

pub fn section_label(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label.into())
}
