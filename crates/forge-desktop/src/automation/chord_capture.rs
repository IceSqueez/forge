use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_XS, ForgePalette, Icon, Spacing,
    ghost_button_with_icon, icon, scaled, spacing, tr,
};
use gpui::{App, ClickEvent, Pixels, Window, div, prelude::*, px};

use crate::automation::hotkey_action_modal::{keycaps, section_caption};

const RECAPTURE_HEIGHT: Pixels = px(32.0);
const DISPLAY_PAD_V: Pixels = px(7.0);
const DISPLAY_PAD_H: Pixels = px(11.0);
const DISPLAY_RADIUS: Pixels = px(7.0);
const DISPLAY_GAP: Pixels = px(7.0);
const LISTEN_GLYPH: Pixels = px(12.0);
const DISPLAY_FS: Pixels = FONT_XS;

pub(crate) struct ChordCapture<'a> {
    pub(crate) capturing: bool,
    pub(crate) caps: Option<&'a str>,
    pub(crate) recapture_id: &'static str,
}

pub(crate) fn chord_capture_field(
    capture: ChordCapture<'_>,
    on_recapture: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> gpui::Div {
    let border = if capture.capturing {
        palette.success
    } else {
        palette.border_input
    };
    let display = div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(scaled(DISPLAY_GAP, density))
        .py(scaled(DISPLAY_PAD_V, density))
        .px(scaled(DISPLAY_PAD_H, density))
        .rounded(DISPLAY_RADIUS)
        .border(BORDER_THIN)
        .border_color(border)
        .bg(palette.shell);
    let display = match (capture.capturing, capture.caps) {
        (true, _) => display
            .child(icon(Icon::Keyboard, LISTEN_GLYPH, palette.success))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(DISPLAY_FS)
                    .text_color(palette.success)
                    .child(tr!("hotkeys_capture_prompt")),
            ),
        (false, Some(caps)) => display.child(keycaps(caps, palette, density, cx)),
        (false, None) => display.child(
            div()
                .font_family(cx.mono_family())
                .text_size(DISPLAY_FS)
                .text_color(palette.text_faint)
                .child(tr!("settings_shortcuts_unbound")),
        ),
    };

    let recapture = ghost_button_with_icon(Icon::Keyboard, tr!("hotkeys_modal_recapture"), palette)
        .height(RECAPTURE_HEIGHT)
        .on_click(capture.recapture_id, on_recapture);

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(section_caption(
            &tr!("hotkeys_modal_section_combo"),
            palette,
            cx,
        ))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Sm, density))
                .child(display)
                .child(div().flex_none().child(recapture)),
        )
}
