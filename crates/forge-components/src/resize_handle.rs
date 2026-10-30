use crate::BORDER_THIN;
use gpui::{
    App, AppContext, Context, CursorStyle, Div, DragMoveEvent, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::palette::ForgePalette;

const RESIZE_VISUAL_W: Pixels = BORDER_THIN;
const RESIZE_HIT_W: Pixels = px(8.0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResizeEdge {
    Left,
    Right,
    Top,
    Bottom,
}

impl ResizeEdge {
    fn is_horizontal(self) -> bool {
        matches!(self, ResizeEdge::Left | ResizeEdge::Right)
    }
}

pub(crate) fn clamp_extent(raw: Pixels, min: Pixels, max: Pixels) -> Pixels {
    raw.min(max).max(min)
}

#[derive(Clone, Copy, Debug)]
pub struct ResizeRange {
    pub min: Pixels,
    pub max: Pixels,
}

struct ResizeGhost;

impl Render for ResizeGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

pub fn install_resize<T: 'static>(
    panel: Div,
    marker: T,
    id: &'static str,
    edge: ResizeEdge,
    range: ResizeRange,
    palette: &ForgePalette,
    handler: impl Fn(&Pixels, &mut Window, &mut App) + 'static,
) -> Div {
    let horizontal = edge.is_horizontal();
    let mut line = div().bg(palette.border_input).group_hover(id, {
        let hover = palette.border_active;
        move |s| s.bg(hover)
    });
    line = if horizontal {
        line.w(RESIZE_VISUAL_W).h_full()
    } else {
        line.h(RESIZE_VISUAL_W).w_full()
    };

    let mut strip = div().absolute().flex();
    strip = match edge {
        ResizeEdge::Left => strip
            .top_0()
            .left_0()
            .h_full()
            .w(RESIZE_HIT_W)
            .justify_start(),
        ResizeEdge::Right => strip
            .top_0()
            .right_0()
            .h_full()
            .w(RESIZE_HIT_W)
            .justify_end(),
        ResizeEdge::Top => strip
            .left_0()
            .top_0()
            .w_full()
            .h(RESIZE_HIT_W)
            .flex_col()
            .justify_start(),
        ResizeEdge::Bottom => strip
            .left_0()
            .bottom_0()
            .w_full()
            .h(RESIZE_HIT_W)
            .flex_col()
            .justify_end(),
    };
    let cursor = if horizontal {
        CursorStyle::ResizeLeftRight
    } else {
        CursorStyle::ResizeUpDown
    };
    let strip = strip
        .group(id)
        .cursor(cursor)
        .id(id)
        .on_drag(marker, |_, _, _, cx| cx.new(|_| ResizeGhost))
        .child(line);

    panel
        .relative()
        .on_drag_move(move |e: &DragMoveEvent<T>, window, cx| {
            let raw = match edge {
                ResizeEdge::Left => e.bounds.right() - e.event.position.x,
                ResizeEdge::Right => e.event.position.x - e.bounds.left(),
                ResizeEdge::Top => e.bounds.bottom() - e.event.position.y,
                ResizeEdge::Bottom => e.event.position.y - e.bounds.top(),
            };
            handler(&clamp_extent(raw, range.min, range.max), window, cx);
        })
        .child(strip)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Pixels = px(200.0);
    const MAX: Pixels = px(400.0);

    #[test]
    fn clamp_extent_holds_the_drag_inside_the_range_at_each_edge() {
        for (raw, expected) in [
            (px(199.0), MIN),
            (MIN, MIN),
            (px(201.0), px(201.0)),
            (px(399.0), px(399.0)),
            (MAX, MAX),
            (px(401.0), MAX),
        ] {
            assert_eq!(clamp_extent(raw, MIN, MAX), expected, "{raw:?}");
        }
    }

    #[test]
    fn clamp_extent_settles_on_the_minimum_when_the_range_is_inverted() {
        for raw in [px(100.0), px(300.0), px(500.0)] {
            assert_eq!(clamp_extent(raw, MAX, MIN), MAX, "{raw:?}");
        }
    }
}
