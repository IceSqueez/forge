use std::borrow::Cow;
use std::ops::Range;

use gpui::{
    Bounds, ClipboardItem, Context, Div, EventEmitter, InteractiveElement, KeyBinding, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, UTF16Selection, Window, actions,
    px,
};

pub(crate) use gpui::EntityInputHandler;

use crate::caret_blink::CaretHost;
use crate::text_buffer::{EditKind, TextBuffer};
use crate::text_edit::{offset_from_utf16, offset_to_utf16, range_from_utf16, range_to_utf16};
use crate::text_input::InputEvent;

const ROW_CENTER: f32 = 0.5;
const EDGE_INSET: Pixels = px(1.0);

actions!(
    forge_text_editing,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        InsertNewline,
        Paste,
        Cut,
        Copy,
        Undo,
        Redo,
    ]
);

pub(crate) fn editing_key_bindings(context: &str) -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, Some(context)),
        KeyBinding::new("delete", Delete, Some(context)),
        KeyBinding::new("left", Left, Some(context)),
        KeyBinding::new("right", Right, Some(context)),
        KeyBinding::new("up", Up, Some(context)),
        KeyBinding::new("down", Down, Some(context)),
        KeyBinding::new("shift-left", SelectLeft, Some(context)),
        KeyBinding::new("shift-right", SelectRight, Some(context)),
        KeyBinding::new("shift-up", SelectUp, Some(context)),
        KeyBinding::new("shift-down", SelectDown, Some(context)),
        KeyBinding::new("home", Home, Some(context)),
        KeyBinding::new("end", End, Some(context)),
        KeyBinding::new("enter", InsertNewline, Some(context)),
        KeyBinding::new("cmd-a", SelectAll, Some(context)),
        KeyBinding::new("ctrl-a", SelectAll, Some(context)),
        KeyBinding::new("cmd-c", Copy, Some(context)),
        KeyBinding::new("ctrl-c", Copy, Some(context)),
        KeyBinding::new("cmd-x", Cut, Some(context)),
        KeyBinding::new("ctrl-x", Cut, Some(context)),
        KeyBinding::new("cmd-v", Paste, Some(context)),
        KeyBinding::new("ctrl-v", Paste, Some(context)),
        KeyBinding::new("cmd-z", Undo, Some(context)),
        KeyBinding::new("ctrl-z", Undo, Some(context)),
        KeyBinding::new("cmd-shift-z", Redo, Some(context)),
        KeyBinding::new("ctrl-shift-z", Redo, Some(context)),
        KeyBinding::new("ctrl-y", Redo, Some(context)),
    ]
}

#[derive(Default)]
pub(crate) struct CaretMotion {
    pub(crate) drag_selecting: bool,
    pub(crate) goal_x: Option<Pixels>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct EditPolicy {
    pub(crate) read_only: bool,
    pub(crate) single_line: bool,
    pub(crate) concealed: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct VerticalMove {
    pub(crate) below: bool,
    pub(crate) extend: bool,
}

pub(crate) struct Composition<'t> {
    pub(crate) replaced_utf16: Option<Range<usize>>,
    pub(crate) text: &'t str,
    pub(crate) selected_utf16_in_text: Option<Range<usize>>,
}

pub(crate) fn vertical_probe_y(
    caret_y: Pixels,
    line_height: Pixels,
    below: bool,
    content_height: Pixels,
) -> Pixels {
    let target = if below {
        caret_y + line_height + line_height * ROW_CENTER
    } else {
        caret_y - line_height * ROW_CENTER
    };
    let last = (content_height - EDGE_INSET).max(px(0.0));
    target.clamp(px(0.0), last)
}

pub(crate) trait TextEditor: CaretHost + EventEmitter<InputEvent> {
    fn buffer(&self) -> &TextBuffer;

    fn buffer_mut(&mut self) -> &mut TextBuffer;

    fn motion(&mut self) -> &mut CaretMotion;

    fn offset_for_position(&self, position: Point<Pixels>) -> usize;

    fn bounds_for_offsets(
        &self,
        range: Range<usize>,
        element_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>>;

    fn policy(&self) -> EditPolicy {
        EditPolicy::default()
    }

    fn reveal_caret(&mut self) {}

    fn sync_after_edit(&mut self) {}

    fn offset_for_input_point(&self, point: Point<Pixels>) -> Option<usize> {
        Some(self.offset_for_position(point))
    }

    fn vertical_target(
        &mut self,
        _below: bool,
        _goal_x: Option<Pixels>,
        _window: &mut Window,
    ) -> Option<(Pixels, usize)> {
        None
    }

    fn cursor_offset(&self) -> usize {
        self.buffer().cursor()
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        offset_to_utf16(self.buffer().as_str(), offset)
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        offset_from_utf16(self.buffer().as_str(), offset)
    }

    fn caret_moved(&mut self, cx: &mut Context<Self>) {
        self.reveal_caret();
        self.caret().wake();
        cx.notify();
    }

    fn edited(&mut self, cx: &mut Context<Self>) {
        self.sync_after_edit();
        self.reveal_caret();
        self.caret().wake();
        cx.emit(InputEvent::Changed(self.buffer().text().into()));
        cx.notify();
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.buffer_mut().move_to(offset);
        self.caret_moved(cx);
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.buffer_mut().select_to(offset);
        self.caret_moved(cx);
    }

    fn navigate(&mut self, step: impl FnOnce(&mut TextBuffer), cx: &mut Context<Self>) {
        self.motion().goal_x = None;
        step(self.buffer_mut());
        self.caret_moved(cx);
    }

    fn edit(&mut self, change: impl FnOnce(&mut TextBuffer), cx: &mut Context<Self>) {
        if self.policy().read_only {
            return;
        }
        self.motion().goal_x = None;
        change(self.buffer_mut());
        self.edited(cx);
    }

    fn step_history(&mut self, step: fn(&mut TextBuffer) -> bool, cx: &mut Context<Self>) {
        if self.policy().read_only || !step(self.buffer_mut()) {
            return;
        }
        self.motion().goal_x = None;
        self.edited(cx);
    }

    fn admitted<'t>(&self, text: &'t str) -> Cow<'t, str> {
        if self.policy().single_line {
            Cow::Owned(text.replace('\n', " "))
        } else {
            Cow::Borrowed(text)
        }
    }

    fn insert_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let text = self.admitted(text);
        self.edit(|buffer| buffer.insert(&text, EditKind::Standalone), cx);
    }

    fn move_vertical(&mut self, step: VerticalMove, window: &mut Window, cx: &mut Context<Self>) {
        let goal_x = self.motion().goal_x;
        let Some((goal_x, target)) = self.vertical_target(step.below, goal_x, window) else {
            return;
        };
        self.motion().goal_x = Some(goal_x);
        if step.extend {
            self.select_to(target, cx);
        } else {
            self.move_to(target, cx);
        }
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(TextBuffer::move_left, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(TextBuffer::move_right, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(TextBuffer::select_left, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(TextBuffer::select_right, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(TextBuffer::select_all, cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        if self.policy().single_line {
            self.navigate(|buffer| buffer.move_to(0), cx);
        } else {
            self.navigate(TextBuffer::move_to_line_start, cx);
        }
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        if self.policy().single_line {
            self.navigate(|buffer| buffer.move_to(buffer.as_str().len()), cx);
        } else {
            self.navigate(TextBuffer::move_to_line_end, cx);
        }
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        let step = VerticalMove {
            below: false,
            extend: false,
        };
        self.move_vertical(step, window, cx);
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        let step = VerticalMove {
            below: true,
            extend: false,
        };
        self.move_vertical(step, window, cx);
    }

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        let step = VerticalMove {
            below: false,
            extend: true,
        };
        self.move_vertical(step, window, cx);
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        let step = VerticalMove {
            below: true,
            extend: true,
        };
        self.move_vertical(step, window, cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(TextBuffer::delete_backward, cx);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(TextBuffer::delete_forward, cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.policy().concealed {
            return;
        }
        if let Some(text) = self.buffer().selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        let policy = self.policy();
        if policy.read_only {
            return;
        }
        let Some(text) = self.buffer().selected_text() else {
            return;
        };
        if !policy.concealed {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        }
        self.edit(|buffer| buffer.insert("", EditKind::Standalone), cx);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.step_history(TextBuffer::undo, cx);
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.step_history(TextBuffer::redo, cx);
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.offset_for_position(event.position);
        let motion = self.motion();
        motion.drag_selecting = true;
        motion.goal_x = None;
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.motion().drag_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.motion().drag_selecting {
            let offset = self.offset_for_position(event.position);
            self.select_to(offset, cx);
        }
    }

    fn input_text_for_range(
        &self,
        range_utf16: &Range<usize>,
        actual_range: &mut Option<Range<usize>>,
    ) -> Option<String> {
        let content = self.buffer().as_str();
        let range = range_from_utf16(content, range_utf16);
        actual_range.replace(range_to_utf16(content, &range));
        content.get(range).map(str::to_string)
    }

    fn input_selection(&self) -> UTF16Selection {
        let buffer = self.buffer();
        UTF16Selection {
            range: range_to_utf16(buffer.as_str(), buffer.selected_range()),
            reversed: buffer.is_reversed(),
        }
    }

    fn input_marked_range(&self) -> Option<Range<usize>> {
        let buffer = self.buffer();
        buffer
            .marked_range()
            .map(|range| range_to_utf16(buffer.as_str(), range))
    }

    fn input_unmark(&mut self) {
        self.buffer_mut().unmark();
    }

    fn input_replace(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        if self.policy().read_only {
            return;
        }
        self.buffer_mut().replace_in_utf16_range(range_utf16, text);
        self.edited(cx);
    }

    fn input_compose(&mut self, composition: Composition<'_>, cx: &mut Context<Self>) {
        if self.policy().read_only {
            return;
        }
        self.buffer_mut().replace_and_mark_in_utf16_range(
            composition.replaced_utf16,
            composition.text,
            composition.selected_utf16_in_text,
        );
        self.edited(cx);
    }

    fn input_bounds_for_range(
        &self,
        range_utf16: &Range<usize>,
        element_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        let range =
            self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end);
        self.bounds_for_offsets(range, element_bounds)
    }

    fn input_index_for_point(&self, point: Point<Pixels>) -> Option<usize> {
        self.offset_for_input_point(point)
            .map(|offset| self.offset_to_utf16(offset))
    }
}

pub(crate) fn with_editing_actions<T: TextEditor>(field: Div, cx: &mut Context<T>) -> Div {
    field
        .on_action(cx.listener(T::backspace))
        .on_action(cx.listener(T::delete))
        .on_action(cx.listener(T::left))
        .on_action(cx.listener(T::right))
        .on_action(cx.listener(T::select_left))
        .on_action(cx.listener(T::select_right))
        .on_action(cx.listener(T::select_all))
        .on_action(cx.listener(T::home))
        .on_action(cx.listener(T::end))
        .on_action(cx.listener(T::copy))
        .on_action(cx.listener(T::cut))
        .on_mouse_down(MouseButton::Left, cx.listener(T::on_mouse_down))
        .on_mouse_up(MouseButton::Left, cx.listener(T::on_mouse_up))
        .on_mouse_up_out(MouseButton::Left, cx.listener(T::on_mouse_up))
        .on_mouse_move(cx.listener(T::on_mouse_move))
}

pub(crate) fn with_multiline_actions<T: TextEditor>(field: Div, cx: &mut Context<T>) -> Div {
    field
        .on_action(cx.listener(T::up))
        .on_action(cx.listener(T::down))
        .on_action(cx.listener(T::select_up))
        .on_action(cx.listener(T::select_down))
        .on_action(cx.listener(T::undo))
        .on_action(cx.listener(T::redo))
}

macro_rules! forward_input_handler {
    ($editor:ty) => {
        impl $crate::text_editing::EntityInputHandler for $editor {
            fn text_for_range(
                &mut self,
                range_utf16: std::ops::Range<usize>,
                actual_range: &mut Option<std::ops::Range<usize>>,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> Option<String> {
                $crate::text_editing::TextEditor::input_text_for_range(
                    self,
                    &range_utf16,
                    actual_range,
                )
            }

            fn selected_text_range(
                &mut self,
                _ignore_disabled_input: bool,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> Option<gpui::UTF16Selection> {
                Some($crate::text_editing::TextEditor::input_selection(self))
            }

            fn marked_text_range(
                &self,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> Option<std::ops::Range<usize>> {
                $crate::text_editing::TextEditor::input_marked_range(self)
            }

            fn unmark_text(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) {
                $crate::text_editing::TextEditor::input_unmark(self);
            }

            fn replace_text_in_range(
                &mut self,
                range_utf16: Option<std::ops::Range<usize>>,
                new_text: &str,
                _: &mut gpui::Window,
                cx: &mut gpui::Context<Self>,
            ) {
                $crate::text_editing::TextEditor::input_replace(self, range_utf16, new_text, cx);
            }

            fn replace_and_mark_text_in_range(
                &mut self,
                range_utf16: Option<std::ops::Range<usize>>,
                new_text: &str,
                new_selected_range_utf16: Option<std::ops::Range<usize>>,
                _: &mut gpui::Window,
                cx: &mut gpui::Context<Self>,
            ) {
                let composition = $crate::text_editing::Composition {
                    replaced_utf16: range_utf16,
                    text: new_text,
                    selected_utf16_in_text: new_selected_range_utf16,
                };
                $crate::text_editing::TextEditor::input_compose(self, composition, cx);
            }

            fn bounds_for_range(
                &mut self,
                range_utf16: std::ops::Range<usize>,
                element_bounds: gpui::Bounds<gpui::Pixels>,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> Option<gpui::Bounds<gpui::Pixels>> {
                $crate::text_editing::TextEditor::input_bounds_for_range(
                    self,
                    &range_utf16,
                    element_bounds,
                )
            }

            fn character_index_for_point(
                &mut self,
                point: gpui::Point<gpui::Pixels>,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> Option<usize> {
                $crate::text_editing::TextEditor::input_index_for_point(self, point)
            }
        }
    };
}

pub(crate) use forward_input_handler;
