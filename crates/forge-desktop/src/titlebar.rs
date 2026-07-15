use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, FONT_SM, FONT_XS, ForgePalette, Radius, radius,
};
use gpui::{Context, FontWeight, MouseButton, MouseDownEvent, Pixels, Window, div, prelude::*, px};

use crate::presentation::{ActivePresentation, Presentation};

const TITLEBAR_HEIGHT: Pixels = px(32.0);
const TITLEBAR_PAD_H: Pixels = px(14.0);
const LOGO_SIZE: Pixels = px(16.0);
const CLUSTER_GAP: Pixels = px(8.0);

pub struct TitleBar;

impl TitleBar {
    pub fn new(cx: &mut Context<Self>) -> Self {
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
            // Active-profile slot: no profile source wired yet — placeholder in the real frame.
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
