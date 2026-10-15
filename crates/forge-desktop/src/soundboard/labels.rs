use forge_components::{FONT_SECTION_LABEL, ForgePalette, mono_family};
use gpui::{SharedString, div, prelude::*, px};

use super::HOTKEY_FS;

pub(super) fn section_label(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FONT_SECTION_LABEL)
        .text_color(palette.text_muted)
        .child(label.into())
}

pub(crate) fn field_lite_label(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .mb(px(5.0))
        .font_family(mono_family())
        .text_size(HOTKEY_FS)
        .text_color(palette.text_muted)
        .child(label.into())
}
