use std::time::Duration;

use gpui::{
    Bounds, CursorStyle, DispatchPhase, Entity, MouseMoveEvent, Pixels, Point, Window, point, px,
};

use crate::text_editing::TextEditor;

pub(crate) const DRAG_SCROLL_TICK: Duration = Duration::from_millis(16);

const STEP_CURVE: f32 = 1.2;
const STEP_DIVISOR: f32 = 16.0;
const MIN_STEP: Pixels = px(1.0);
const MAX_STEP: Pixels = px(120.0);
const MAX_MARGIN_SHARE: f32 = 3.0;
const LAST_ROW_INSET: Pixels = px(1.0);
const LINE_CENTER: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextEdge {
    Start,
    End,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DragViewport {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) axis: ScrollAxis,
    pub(crate) scroll: Pixels,
    pub(crate) max_scroll: Pixels,
    pub(crate) line_height: Pixels,
}

pub(crate) fn drag_scroll_speed(overshoot: Pixels) -> Pixels {
    let eased = f32::from(overshoot).max(0.0).powf(STEP_CURVE) / STEP_DIVISOR;
    px(eased).max(MIN_STEP).min(MAX_STEP)
}

impl DragViewport {
    fn extent(&self) -> (Pixels, Pixels) {
        match self.axis {
            ScrollAxis::Horizontal => (self.bounds.left(), self.bounds.right()),
            ScrollAxis::Vertical => (self.bounds.top(), self.bounds.bottom()),
        }
    }

    fn along(&self, pointer: Point<Pixels>) -> Pixels {
        match self.axis {
            ScrollAxis::Horizontal => pointer.x,
            ScrollAxis::Vertical => pointer.y,
        }
    }

    pub(crate) fn scroll_step(&self, pointer: Point<Pixels>) -> Pixels {
        let (start, end) = self.extent();
        let margin = self.line_height.min((end - start) / MAX_MARGIN_SHARE);
        let along = self.along(pointer);
        if along < start + margin {
            -drag_scroll_speed(start + margin - along).min(self.scroll)
        } else if along > end - margin {
            drag_scroll_speed(along - end + margin).min(self.max_scroll - self.scroll)
        } else {
            px(0.0)
        }
    }

    pub(crate) fn document_edge_past(&self, pointer: Point<Pixels>) -> Option<TextEdge> {
        let (start, end) = self.extent();
        let along = self.along(pointer);
        if along < start && self.scroll <= px(0.0) {
            Some(TextEdge::Start)
        } else if along > end && self.scroll >= self.max_scroll {
            Some(TextEdge::End)
        } else {
            None
        }
    }

    pub(crate) fn pinned(&self, pointer: Point<Pixels>) -> Point<Pixels> {
        let x = pointer.x.max(self.bounds.left()).min(self.bounds.right());
        match self.axis {
            ScrollAxis::Horizontal => {
                point(x, self.bounds.top() + self.bounds.size.height * LINE_CENTER)
            }
            ScrollAxis::Vertical => point(
                x,
                pointer
                    .y
                    .min(self.bounds.bottom() - LAST_ROW_INSET)
                    .max(self.bounds.top()),
            ),
        }
    }
}

pub(crate) fn paint_drag_tracking<T: TextEditor>(editor: &Entity<T>, window: &mut Window) {
    window.set_window_cursor_style(CursorStyle::IBeam);
    let editor = editor.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
        if phase == DispatchPhase::Capture {
            editor.update(cx, |editor, cx| {
                editor.drag_pointer_moved(event.position, event.pressed_button, cx);
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use gpui::size;

    use super::*;

    const TOP: f32 = 100.0;
    const BOTTOM: f32 = 300.0;
    const LEFT: f32 = 100.0;
    const RIGHT: f32 = 400.0;
    const MARGIN: f32 = 20.0;
    const MID_X: f32 = 250.0;
    const MID_Y: f32 = 200.0;
    const FAR_AWAY: f32 = -900.0;

    fn viewport(axis: ScrollAxis, scroll: f32, max_scroll: f32) -> DragViewport {
        DragViewport {
            bounds: Bounds::new(
                point(px(LEFT), px(TOP)),
                size(px(RIGHT - LEFT), px(BOTTOM - TOP)),
            ),
            axis,
            scroll: px(scroll),
            max_scroll: px(max_scroll),
            line_height: px(MARGIN),
        }
    }

    fn vertical(scroll: f32, max_scroll: f32) -> DragViewport {
        viewport(ScrollAxis::Vertical, scroll, max_scroll)
    }

    fn horizontal(scroll: f32, max_scroll: f32) -> DragViewport {
        viewport(ScrollAxis::Horizontal, scroll, max_scroll)
    }

    fn at_y(y: f32) -> Point<Pixels> {
        point(px(MID_X), px(y))
    }

    fn at_x(x: f32) -> Point<Pixels> {
        point(px(x), px(MID_Y))
    }

    #[test]
    fn the_speed_never_stalls_grows_with_overshoot_and_stops_at_the_cap() {
        for overshoot in [-50.0, 0.0, 1.0] {
            assert_eq!(drag_scroll_speed(px(overshoot)), px(1.0), "{overshoot}");
        }
        assert!((f32::from(drag_scroll_speed(px(100.0))) - 15.7).abs() < 0.05);
        assert_eq!(drag_scroll_speed(px(1000.0)), px(120.0));
        assert_eq!(drag_scroll_speed(px(100_000.0)), px(120.0));

        let speeds: Vec<f32> = (0..1000)
            .map(|overshoot| f32::from(drag_scroll_speed(px(overshoot as f32))))
            .collect();
        assert!(speeds.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn scrolling_starts_one_pixel_inside_the_margin_and_never_leaves_the_document() {
        for (y, scroll, max_scroll, expected) in [
            (MID_Y, 50.0, 300.0, 0.0),
            (TOP + MARGIN, 50.0, 300.0, 0.0),
            (BOTTOM - MARGIN, 50.0, 300.0, 0.0),
            (TOP + MARGIN - 1.0, 50.0, 300.0, -1.0),
            (BOTTOM - MARGIN + 1.0, 50.0, 300.0, 1.0),
            (TOP + MARGIN - 1.0, 0.5, 300.0, -0.5),
            (TOP + MARGIN - 1.0, 0.0, 300.0, 0.0),
            (BOTTOM - MARGIN + 1.0, 299.5, 300.0, 0.5),
            (BOTTOM - MARGIN + 1.0, 300.0, 300.0, 0.0),
            (FAR_AWAY, 500.0, 1000.0, -120.0),
            (BOTTOM - FAR_AWAY, 500.0, 1000.0, 120.0),
        ] {
            assert_eq!(
                vertical(scroll, max_scroll).scroll_step(at_y(y)),
                px(expected),
                "pointer {y} at scroll {scroll} of {max_scroll}",
            );
        }
    }

    #[test]
    fn the_step_follows_only_the_axis_the_field_scrolls_on() {
        let across_vertical = point(px(FAR_AWAY), px(MID_Y));
        let across_horizontal = point(px(MID_X), px(FAR_AWAY));

        assert_eq!(vertical(50.0, 300.0).scroll_step(across_vertical), px(0.0));
        assert_eq!(
            horizontal(50.0, 300.0).scroll_step(across_horizontal),
            px(0.0)
        );
        assert_eq!(
            horizontal(50.0, 300.0).scroll_step(at_x(LEFT + MARGIN - 1.0)),
            px(-1.0)
        );
        assert_eq!(
            horizontal(50.0, 300.0).scroll_step(at_x(RIGHT - MARGIN + 1.0)),
            px(1.0)
        );
    }

    #[test]
    fn a_short_field_keeps_a_middle_third_that_does_not_scroll() {
        let mut short = vertical(50.0, 300.0);
        short.bounds.size.height = px(30.0);
        let third = TOP + 10.0;

        assert_eq!(short.scroll_step(at_y(third)), px(0.0));
        assert_eq!(short.scroll_step(at_y(third - 1.0)), px(-1.0));
    }

    #[test]
    fn the_document_edge_is_past_only_outside_the_field_with_nothing_left_to_scroll() {
        for (y, scroll, max_scroll, expected) in [
            (TOP - 1.0, 0.0, 300.0, Some(TextEdge::Start)),
            (TOP - 1.0, 0.5, 300.0, None),
            (TOP, 0.0, 300.0, None),
            (BOTTOM + 1.0, 300.0, 300.0, Some(TextEdge::End)),
            (BOTTOM + 1.0, 299.5, 300.0, None),
            (BOTTOM, 300.0, 300.0, None),
            (MID_Y, 0.0, 0.0, None),
            (TOP - 1.0, 0.0, 0.0, Some(TextEdge::Start)),
            (BOTTOM + 1.0, 0.0, 0.0, Some(TextEdge::End)),
        ] {
            assert_eq!(
                vertical(scroll, max_scroll).document_edge_past(at_y(y)),
                expected,
                "pointer {y} at scroll {scroll} of {max_scroll}",
            );
        }
    }

    #[test]
    fn a_horizontal_field_reads_its_document_edge_from_the_pointer_x() {
        let field = horizontal(0.0, 0.0);

        assert_eq!(
            field.document_edge_past(at_x(LEFT - 1.0)),
            Some(TextEdge::Start)
        );
        assert_eq!(
            field.document_edge_past(at_x(RIGHT + 1.0)),
            Some(TextEdge::End)
        );
        assert_eq!(field.document_edge_past(at_y(FAR_AWAY)), None);
    }

    #[test]
    fn a_vertical_field_pins_the_pointer_to_a_row_it_can_hit() {
        let field = vertical(0.0, 0.0);

        for (pointer, expected) in [
            (point(px(150.0), px(150.0)), point(px(150.0), px(150.0))),
            (point(px(LEFT - 5.0), px(150.0)), point(px(LEFT), px(150.0))),
            (
                point(px(RIGHT + 5.0), px(150.0)),
                point(px(RIGHT), px(150.0)),
            ),
            (point(px(150.0), px(TOP - 5.0)), point(px(150.0), px(TOP))),
            (
                point(px(150.0), px(BOTTOM + 5.0)),
                point(px(150.0), px(BOTTOM - 1.0)),
            ),
        ] {
            assert_eq!(field.pinned(pointer), expected);
        }
    }

    #[test]
    fn a_horizontal_field_pins_the_pointer_to_its_one_line_at_any_height() {
        let field = horizontal(0.0, 0.0);

        for (pointer, expected_x) in [
            (point(px(150.0), px(FAR_AWAY)), 150.0),
            (point(px(LEFT - 5.0), px(BOTTOM + 500.0)), LEFT),
            (point(px(RIGHT + 5.0), px(TOP)), RIGHT),
        ] {
            assert_eq!(field.pinned(pointer), point(px(expected_x), px(MID_Y)));
        }
    }
}
