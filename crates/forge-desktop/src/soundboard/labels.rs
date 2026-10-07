use forge_components::{ForgePalette, mono_family};
use gpui::{Pixels, SharedString, div, prelude::*, px};

use super::HOTKEY_FS;

const SECTION_LABEL_FS: Pixels = px(9.5);

pub(super) fn section_label(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(SECTION_LABEL_FS)
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
