use forge_components::{ActivePresentation, Density, FONT_SECTION_LABEL, ForgePalette, scaled};
use gpui::{App, SharedString, div, prelude::*, px};

use super::HOTKEY_FS;

const FIELD_LABEL_MB: gpui::Pixels = px(5.0);

pub(super) fn section_label(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
    cx: &App,
) -> impl IntoElement {
    div()
        .font_family(cx.mono_family())
        .text_size(FONT_SECTION_LABEL)
        .text_color(palette.text_muted)
        .child(label.into())
}

pub(crate) fn field_lite_label(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> impl IntoElement {
    div()
        .mb(scaled(FIELD_LABEL_MB, density))
        .font_family(cx.mono_family())
        .text_size(HOTKEY_FS)
        .text_color(palette.text_muted)
        .child(label.into())
}
