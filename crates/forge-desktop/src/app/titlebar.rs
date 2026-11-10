#[cfg(not(target_os = "macos"))]
use crate::app::actions::Quit;
use forge_components::{BORDER_THIN, FONT_SM, FONT_XS, ForgePalette, Radius, radius, scaled};
#[cfg(not(target_os = "macos"))]
use forge_components::{Icon, with_alpha};
use gpui::{
    App, ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Window, WindowControlArea, div, prelude::*, px,
};
#[cfg(not(target_os = "macos"))]
use gpui::{Rgba, svg};

use forge_components::{ActivePresentation, Presentation};

pub(crate) const TITLEBAR_HEIGHT: Pixels = px(32.0);
const LOGO_SIZE: Pixels = px(16.0);
const CLUSTER_GAP: Pixels = px(8.0);
const DOUBLE_CLICK_COUNT: usize = 2;
#[cfg(not(target_os = "macos"))]
const WINDOW_CONTROL_WIDTH: Pixels = px(46.0);
#[cfg(not(target_os = "macos"))]
const WINDOW_CONTROL_GLYPH: Pixels = px(13.0);
#[cfg(not(target_os = "macos"))]
const WINDOW_CONTROL_HOVER_ALPHA: f32 = 0.08;
const BRAND_INITIAL: &str = "F";

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Copy)]
enum WindowControl {
    Minimize,
    Maximize,
    Close,
}

#[cfg(not(target_os = "macos"))]
impl WindowControl {
    fn id(self) -> &'static str {
        match self {
            Self::Minimize => "titlebar-minimize",
            Self::Maximize => "titlebar-maximize",
            Self::Close => "titlebar-close",
        }
    }

    fn group(self) -> &'static str {
        match self {
            Self::Minimize => "titlebar-minimize-group",
            Self::Maximize => "titlebar-maximize-group",
            Self::Close => "titlebar-close-group",
        }
    }

    fn control_area(self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Maximize => WindowControlArea::Max,
            Self::Close => WindowControlArea::Close,
        }
    }

    fn glyph(self, maximized: bool) -> Icon {
        match self {
            Self::Minimize => Icon::Minus,
            Self::Maximize if maximized => Icon::Copy,
            Self::Maximize => Icon::Square,
            Self::Close => Icon::X,
        }
    }
}

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

#[cfg(not(target_os = "macos"))]
fn window_controls(palette: &ForgePalette, window: &Window) -> impl IntoElement {
    let supported = window.window_controls();
    let maximized = window.is_maximized();
    let mut controls = div()
        .id("titlebar-window-controls")
        .absolute()
        .top_0()
        .right_0()
        .flex_none()
        .h_full()
        .flex()
        .items_center()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());

    if supported.minimize {
        controls = controls.child(window_control(
            WindowControl::Minimize,
            window.is_minimizable(),
            maximized,
            palette,
        ));
    }
    if supported.maximize {
        controls = controls.child(window_control(
            WindowControl::Maximize,
            window.is_resizable(),
            maximized,
            palette,
        ));
    }
    controls.child(window_control(
        WindowControl::Close,
        true,
        maximized,
        palette,
    ))
}

#[cfg(not(target_os = "macos"))]
fn window_control(
    control: WindowControl,
    enabled: bool,
    maximized: bool,
    palette: &ForgePalette,
) -> impl IntoElement {
    let idle_ink = palette.text_secondary;
    let hover_ink = if matches!(control, WindowControl::Close) {
        Rgba {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        }
    } else {
        palette.text_primary
    };
    let hover_fill = if matches!(control, WindowControl::Close) {
        palette.danger
    } else {
        with_alpha(palette.text_primary, WINDOW_CONTROL_HOVER_ALPHA)
    };
    let group = control.group();

    div()
        .id(control.id())
        .group(group)
        .flex_none()
        .w(WINDOW_CONTROL_WIDTH)
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .window_control_area(control.control_area())
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(hover_fill))
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_move(|_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            if !enabled {
                return;
            }
            match control {
                WindowControl::Minimize => window.minimize_window(),
                WindowControl::Maximize => window.zoom_window(),
                WindowControl::Close => window.dispatch_action(Box::new(Quit), cx),
            }
        })
        .child(
            svg()
                .flex_none()
                .size(WINDOW_CONTROL_GLYPH)
                .path(control.glyph(maximized).path())
                .text_color(idle_ink)
                .when(enabled, move |glyph| {
                    glyph.group_hover(group, move |glyph| glyph.text_color(hover_ink))
                }),
        )
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();

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

        let titlebar = div()
            .w_full()
            .h(TITLEBAR_HEIGHT)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .relative()
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
            .child(cluster);

        #[cfg(not(target_os = "macos"))]
        let titlebar = titlebar.child(window_controls(&palette, window));
        #[cfg(target_os = "macos")]
        let _ = window;

        titlebar
    }
}
