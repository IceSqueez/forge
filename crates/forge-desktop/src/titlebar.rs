use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, FONT_SM, FONT_XS, ForgePalette, Radius, radius,
};
use gpui::{Context, FontWeight, MouseButton, MouseDownEvent, Pixels, Window, div, prelude::*, px};

use crate::presentation::{ActivePresentation, Presentation};

/// Fixed drawn height of the title bar, matching the design's 32px branded chrome
/// strip. A title bar is fixed, density-neutral chrome, so its geometry is carried
/// as hand-tuned literals (mirroring the kit footer's precedent) rather than
/// snapped to the density-scaled `Spacing` steps.
const TITLEBAR_HEIGHT: Pixels = px(32.0);
/// Side padding of the whole bar; also the macOS traffic-light inset.
const TITLEBAR_PAD_H: Pixels = px(14.0);
/// Square-ish brand mark box, 16px per the design.
const LOGO_SIZE: Pixels = px(16.0);
/// Gap inside the centered identity cluster (logo · name · profile).
const CLUSTER_GAP: Pixels = px(8.0);

/// Custom transparent title bar rendered as its own child view-entity. The window opens with `appears_transparent`, so on macOS/Windows
/// this bar replaces the OS title bar; on macOS the traffic lights inset into the
/// left padding. Dragging the bar moves the window (Wayland/X11 compositor move).
/// It reads the active palette from the presentation `Global` and holds no state.
pub struct TitleBar;

impl TitleBar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Repaint on theme/density switch (the global is replaced on change).
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        Self
    }

    fn logo(palette: &ForgePalette) -> impl IntoElement {
        div()
            .flex_none()
            .size(LOGO_SIZE)
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::BOLD)
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child("F"),
            )
    }
}

impl Render for TitleBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let cluster = div()
            .flex()
            .items_center()
            .gap(CLUSTER_GAP)
            .child(Self::logo(&palette))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Forge"),
            )
            .child(
                div()
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child("—"),
            )
            // Active-profile slot: no profile source is wired yet, so it renders
            // the missing-data placeholder inside the real frame.
            .child(
                div()
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child("—"),
            );

        div()
            .w_full()
            .h(TITLEBAR_HEIGHT)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .px(TITLEBAR_PAD_H)
            .bg(palette.shell)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, window, _| window.start_window_move()),
            )
            .child(cluster)
    }
}
