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

    if !tiling.top && !tiling.left && near_top && near_left {
        return Some(ResizeEdge::TopLeft);
    }
    if !tiling.top && !tiling.right && near_top && near_right {
        return Some(ResizeEdge::TopRight);
    }
    if !tiling.bottom && !tiling.left && near_bottom && near_left {
        return Some(ResizeEdge::BottomLeft);
    }
    if !tiling.bottom && !tiling.right && near_bottom && near_right {
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

#[cfg(test)]
mod tests {
    use gpui::{Corners, Pixels, Point, ResizeEdge, Size, Tiling, point, px, size};

    use super::{CLIENT_CORNER, SQUARE_CORNER, resize_edge, rounded_corners};

    fn window() -> Size<Pixels> {
        size(px(800.0), px(600.0))
    }

    fn at(x: f32, y: f32) -> Point<Pixels> {
        point(px(x), px(y))
    }

    fn left_tiled() -> Tiling {
        Tiling {
            left: true,
            ..Tiling::default()
        }
    }

    #[test]
    fn a_floating_window_resizes_from_each_edge_and_corner_with_corners_winning() {
        for (position, expected) in [
            (at(400.0, 5.0), ResizeEdge::Top),
            (at(400.0, 595.0), ResizeEdge::Bottom),
            (at(5.0, 300.0), ResizeEdge::Left),
            (at(795.0, 300.0), ResizeEdge::Right),
            (at(5.0, 5.0), ResizeEdge::TopLeft),
            (at(795.0, 5.0), ResizeEdge::TopRight),
            (at(5.0, 595.0), ResizeEdge::BottomLeft),
            (at(795.0, 595.0), ResizeEdge::BottomRight),
        ] {
            assert_eq!(
                resize_edge(position, window(), Tiling::default()),
                Some(expected),
                "at {position:?}"
            );
        }
    }

    #[test]
    fn past_the_shadow_strip_only_the_corner_grab_still_resizes() {
        for (position, expected) in [
            (at(9.0, 300.0), Some(ResizeEdge::Left)),
            (at(10.0, 300.0), None),
            (at(14.0, 300.0), None),
            (at(14.0, 14.0), Some(ResizeEdge::TopLeft)),
            (at(786.0, 586.0), Some(ResizeEdge::BottomRight)),
            (at(15.0, 15.0), None),
            (at(400.0, 300.0), None),
        ] {
            assert_eq!(
                resize_edge(position, window(), Tiling::default()),
                expected,
                "at {position:?}"
            );
        }
    }

    #[test]
    fn a_fully_tiled_window_offers_no_resize_edge_anywhere_on_its_border() {
        for position in [
            at(400.0, 5.0),
            at(400.0, 595.0),
            at(5.0, 300.0),
            at(795.0, 300.0),
            at(5.0, 5.0),
            at(795.0, 5.0),
            at(5.0, 595.0),
            at(795.0, 595.0),
        ] {
            assert_eq!(
                resize_edge(position, window(), Tiling::tiled()),
                None,
                "at {position:?}"
            );
        }
    }

    #[test]
    fn a_tiled_side_offers_no_edge_while_the_free_sides_still_resize() {
        assert_eq!(
            (
                resize_edge(at(5.0, 300.0), window(), left_tiled()),
                resize_edge(at(795.0, 300.0), window(), left_tiled()),
                resize_edge(at(400.0, 5.0), window(), left_tiled()),
            ),
            (None, Some(ResizeEdge::Right), Some(ResizeEdge::Top))
        );
    }

    #[test]
    fn a_tiled_side_squares_the_two_corners_it_touches() {
        let rounded = CLIENT_CORNER;
        let square = SQUARE_CORNER;
        for (tiling, expected) in [
            (
                Tiling::default(),
                Corners {
                    top_left: rounded,
                    top_right: rounded,
                    bottom_left: rounded,
                    bottom_right: rounded,
                },
            ),
            (
                left_tiled(),
                Corners {
                    top_left: square,
                    top_right: rounded,
                    bottom_left: square,
                    bottom_right: rounded,
                },
            ),
            (
                Tiling {
                    top: true,
                    ..Tiling::default()
                },
                Corners {
                    top_left: square,
                    top_right: square,
                    bottom_left: rounded,
                    bottom_right: rounded,
                },
            ),
            (
                Tiling {
                    bottom: true,
                    right: true,
                    ..Tiling::default()
                },
                Corners {
                    top_left: rounded,
                    top_right: square,
                    bottom_left: square,
                    bottom_right: square,
                },
            ),
        ] {
            assert_eq!(rounded_corners(tiling), expected, "for {tiling:?}");
        }
    }
}
