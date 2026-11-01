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
