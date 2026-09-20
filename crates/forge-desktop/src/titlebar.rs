use forge_components::{BORDER_THIN, FONT_SM, FONT_XS, ForgePalette, Radius, body_family, radius};
use gpui::{
    ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Window, WindowControlArea, div, prelude::*, px,
};

use crate::presentation::{ActivePresentation, Presentation};

pub const TITLEBAR_HEIGHT: Pixels = px(32.0);
const TITLEBAR_PAD_H: Pixels = px(14.0);
const LOGO_SIZE: Pixels = px(16.0);
const CLUSTER_GAP: Pixels = px(8.0);
const DOUBLE_CLICK_COUNT: usize = 2;

pub struct TitleBar {
    should_move: bool,
}

impl TitleBar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        Self { should_move: false }
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
                    .font_family(body_family())
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
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Forge"),
            )
            .child(
                div()
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child("-"),
            )
            // Active-profile slot: no profile source wired yet - placeholder in the real frame.
            .child(
                div()
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child("-"),
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
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, _| {
                this.should_move = false;
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| {
                    this.should_move = false;
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, _| {
                    this.should_move = true;
                }),
            )
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, window, _| {
                if this.should_move {
                    this.should_move = false;
                    window.start_window_move();
                }
            }))
            .id("titlebar")
            .on_click(cx.listener(|_, event: &ClickEvent, window, _| {
                if event.click_count() == DOUBLE_CLICK_COUNT {
                    if cfg!(target_os = "macos") {
                        window.titlebar_double_click();
                    } else if cfg!(target_os = "linux") {
                        window.zoom_window();
                    }
                }
            }))
            .child(cluster)
    }
}
