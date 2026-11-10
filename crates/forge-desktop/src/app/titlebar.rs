use forge_components::{
    BORDER_THIN, FONT_SM, FONT_XS, ForgePalette, Radius, WindowControlsLayout, client_corners,
    radius, scaled, window_controls,
};
use gpui::{
    App, ClickEvent, Context, Decorations, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Window, WindowControlArea, div, prelude::*, px,
};

use crate::app::actions::Quit;
use crate::app::window_chrome::{forge_titlebar_shown, forge_window_controls_shown};

use forge_components::{ActivePresentation, Presentation};

pub(crate) const TITLEBAR_HEIGHT: Pixels = px(32.0);
const TITLEBAR_PAD_H: Pixels = px(14.0);
const LOGO_SIZE: Pixels = px(16.0);
const CLUSTER_GAP: Pixels = px(8.0);
const DOUBLE_CLICK_COUNT: usize = 2;
const BRAND_INITIAL: &str = "F";

pub(crate) struct TitleBar {
    should_move: bool,
}

impl TitleBar {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        Self { should_move: false }
    }
}

pub(crate) fn brand_mark(
    size: Pixels,
    corner: Pixels,
    glyph: Pixels,
    palette: &ForgePalette,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex_none()
        .size(size)
        .flex()
        .items_center()
        .justify_center()
        .rounded(corner)
        .bg(palette.brand)
        .child(
            div()
                .font_family(cx.body_family())
                .font_weight(FontWeight::BOLD)
                .text_size(glyph)
                .text_color(palette.shell)
                .child(BRAND_INITIAL),
        )
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !forge_titlebar_shown(window, cx) {
            return div().into_any_element();
        }
        let density = cx.density();
        let palette = cx.palette();
        let corners = client_corners(window);
        let controls = forge_window_controls_shown(window, cx).then(|| {
            div()
                .absolute()
                .top_0()
                .right_0()
                .h_full()
                .child(window_controls(
                    WindowControlsLayout::native(),
                    &palette,
                    |window, cx| window.dispatch_action(Box::new(Quit), cx),
                ))
        });

        let cluster = div()
            .flex()
            .items_center()
            .gap(scaled(CLUSTER_GAP, density))
            .child(brand_mark(
                LOGO_SIZE,
                radius(Radius::Sm),
                FONT_XS,
                &palette,
                cx,
            ))
            .child(
                div()
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Forge"),
            );

        div()
            .w_full()
            .h(TITLEBAR_HEIGHT)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .relative()
            .px(scaled(TITLEBAR_PAD_H, density))
            .rounded_tl(corners.top_left)
            .rounded_tr(corners.top_right)
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
            .on_mouse_down(MouseButton::Right, |event: &MouseDownEvent, window, _| {
                if window_menu_available(window) {
                    window.show_window_menu(event.position);
                }
            })
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
            .children(controls)
            .into_any_element()
    }
}

fn window_menu_available(window: &Window) -> bool {
    cfg!(target_os = "linux")
        && matches!(window.window_decorations(), Decorations::Client { .. })
        && window.window_controls().window_menu
}
