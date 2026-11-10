use gpui::{
    AnyElement, App, Bounds, BoxShadow, Corners, CursorStyle, Decorations, Global, HitboxBehavior,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point, ResizeEdge, Size,
    Styled, Tiling, Window, canvas, div, prelude::FluentBuilder, px,
};

use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, Radius, radius};

pub const CLIENT_SHADOW: Pixels = px(10.0);
const CLIENT_CORNER: Pixels = radius(Radius::Md);
const SQUARE_CORNER: Pixels = px(0.0);
const NO_INSET: Pixels = px(0.0);
const CORNER_GRAB_SCALE: f32 = 1.5;
const SHADOW_BLUR_SHARE: f32 = 0.5;

struct HoveredResizeEdge(Option<ResizeEdge>);

impl Global for HoveredResizeEdge {}

pub fn client_frame_tiling(window: &Window) -> Option<Tiling> {
    match window.window_decorations() {
        Decorations::Server => None,
        Decorations::Client { .. } if window.is_fullscreen() || window.is_maximized() => {
            Some(Tiling::tiled())
        }
        Decorations::Client { tiling } => Some(tiling),
    }
}

pub fn client_corners(window: &Window) -> Corners<Pixels> {
    client_frame_tiling(window)
        .map(rounded_corners)
        .unwrap_or_default()
}

pub fn rounded_corners(tiling: Tiling) -> Corners<Pixels> {
    let corner = |vertical: bool, horizontal: bool| {
        if vertical || horizontal {
            SQUARE_CORNER
        } else {
            CLIENT_CORNER
        }
    };
    Corners {
        top_left: corner(tiling.top, tiling.left),
        top_right: corner(tiling.top, tiling.right),
        bottom_left: corner(tiling.bottom, tiling.left),
        bottom_right: corner(tiling.bottom, tiling.right),
    }
}

pub fn resize_edge(
    position: Point<Pixels>,
    window_size: Size<Pixels>,
    tiling: Tiling,
) -> Option<ResizeEdge> {
    let grab = CLIENT_SHADOW * CORNER_GRAB_SCALE;
    let interior = Bounds::new(Point::default(), window_size).inset(grab);
    if interior.contains(&position) {
        return None;
    }

    let near_left = position.x < grab;
    let near_right = position.x > window_size.width - grab;
    let near_top = position.y < grab;
    let near_bottom = position.y > window_size.height - grab;

    if !tiling.top && near_top && near_left {
        return Some(ResizeEdge::TopLeft);
    }
    if !tiling.top && near_top && near_right {
        return Some(ResizeEdge::TopRight);
    }
    if !tiling.bottom && near_bottom && near_left {
        return Some(ResizeEdge::BottomLeft);
    }
    if !tiling.bottom && near_bottom && near_right {
        return Some(ResizeEdge::BottomRight);
    }

    if !tiling.top && position.y < CLIENT_SHADOW {
        Some(ResizeEdge::Top)
    } else if !tiling.bottom && position.y > window_size.height - CLIENT_SHADOW {
        Some(ResizeEdge::Bottom)
    } else if !tiling.left && position.x < CLIENT_SHADOW {
        Some(ResizeEdge::Left)
    } else if !tiling.right && position.x > window_size.width - CLIENT_SHADOW {
        Some(ResizeEdge::Right)
    } else {
        None
    }
}

fn edge_cursor(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

fn track_hovered_edge(edge: Option<ResizeEdge>, window: &mut Window, cx: &mut App) {
    let previous = cx
        .try_global::<HoveredResizeEdge>()
        .and_then(|hovered| hovered.0);
    if previous != edge {
        cx.set_global(HoveredResizeEdge(edge));
        window.refresh();
    }
}

pub fn window_frame(
    content: impl IntoElement,
    palette: &ForgePalette,
    window: &mut Window,
) -> AnyElement {
    let Some(tiling) = client_frame_tiling(window) else {
        window.set_client_inset(NO_INSET);
        return div().size_full().child(content).into_any_element();
    };
    window.set_client_inset(CLIENT_SHADOW);

    let corners = rounded_corners(tiling);
    let resizable = window.is_resizable();
    let floating = !tiling.is_tiled();

    let surface = div()
        .size_full()
        .cursor(CursorStyle::Arrow)
        .rounded_tl(corners.top_left)
        .rounded_tr(corners.top_right)
        .rounded_bl(corners.bottom_left)
        .rounded_br(corners.bottom_right)
        .bg(palette.base)
        .border_color(palette.border_regular)
        .when(!tiling.top, |surface| surface.border_t(BORDER_THIN))
        .when(!tiling.bottom, |surface| surface.border_b(BORDER_THIN))
        .when(!tiling.left, |surface| surface.border_l(BORDER_THIN))
        .when(!tiling.right, |surface| surface.border_r(BORDER_THIN))
        .when(floating, |surface| {
            surface.shadow(vec![
                BoxShadow::new(SQUARE_CORNER, SQUARE_CORNER, palette.scrim.into())
                    .blur_radius(CLIENT_SHADOW * SHADOW_BLUR_SHARE),
            ])
        })
        .on_mouse_move(|_, _, cx| cx.stop_propagation())
        .child(content);

    div()
        .id("window-frame")
        .size_full()
        .when(!tiling.top, |frame| frame.pt(CLIENT_SHADOW))
        .when(!tiling.bottom, |frame| frame.pb(CLIENT_SHADOW))
        .when(!tiling.left, |frame| frame.pl(CLIENT_SHADOW))
        .when(!tiling.right, |frame| frame.pr(CLIENT_SHADOW))
        .when(resizable, |frame| {
            frame
                .on_mouse_move(move |event, window, cx| {
                    let edge = resize_edge(event.position, window.viewport_size(), tiling);
                    track_hovered_edge(edge, window, cx);
                })
                .on_mouse_down(MouseButton::Left, move |event, window, _| {
                    if let Some(edge) = resize_edge(event.position, window.viewport_size(), tiling)
                    {
                        window.start_window_resize(edge);
                    }
                })
        })
        .child(surface)
        .when(resizable, |frame| {
            frame.child(
                canvas(
                    |_, window, _| {
                        window.insert_hitbox(
                            Bounds::new(Point::default(), window.viewport_size()),
                            HitboxBehavior::Normal,
                        )
                    },
                    move |_, hitbox, window, _| {
                        if let Some(edge) =
                            resize_edge(window.mouse_position(), window.viewport_size(), tiling)
                        {
                            window.set_cursor_style(edge_cursor(edge), &hitbox);
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
        })
        .into_any_element()
}
