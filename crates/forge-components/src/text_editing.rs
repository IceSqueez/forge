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
        if self.policy().concealed {
            return None;
        }
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
        let policy = self.policy();
        if policy.read_only || policy.concealed {
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::highlight::Language;
    use crate::{CodeEditor, TextArea, TextInput};
    use gpui::{Render, TestAppContext};

    trait Harness: TextEditor + EntityInputHandler + Render + Sized + 'static {
        const NAME: &'static str;

        fn fresh(cx: &mut Context<Self>) -> Self;

        fn load(&mut self, content: &str, cx: &mut Context<Self>);
    }

    impl Harness for TextInput {
        const NAME: &'static str = "text input";

        fn fresh(cx: &mut Context<Self>) -> Self {
            TextInput::new("placeholder", cx)
        }

        fn load(&mut self, content: &str, cx: &mut Context<Self>) {
            self.set_content(content.to_owned(), cx);
        }
    }

    impl Harness for TextArea {
        const NAME: &'static str = "text area";

        fn fresh(cx: &mut Context<Self>) -> Self {
            TextArea::new("placeholder", cx)
        }

        fn load(&mut self, content: &str, cx: &mut Context<Self>) {
            self.set_content(content.to_owned(), cx);
        }
    }

    impl Harness for CodeEditor {
        const NAME: &'static str = "code editor";

        fn fresh(cx: &mut Context<Self>) -> Self {
            CodeEditor::new(Language::Rhai, "placeholder", cx)
        }

        fn load(&mut self, content: &str, cx: &mut Context<Self>) {
            self.set_content(content.to_owned(), cx);
        }
    }

    fn on<T: Harness, R>(
        cx: &mut TestAppContext,
        content: &str,
        act: impl FnOnce(&mut T, &mut Window, &mut Context<T>) -> R,
    ) -> R {
        let window = cx.add_window(|_window, cx| T::fresh(cx));
        window
            .update(cx, |editor, window, cx| {
                editor.load(content, cx);
                act(editor, window, cx)
            })
            .unwrap()
    }

    #[derive(Debug, PartialEq)]
    struct Shape {
        text: String,
        selected: Range<usize>,
        marked: Option<Range<usize>>,
    }

    fn shape(editor: &impl TextEditor) -> Shape {
        let buffer = editor.buffer();
        Shape {
            text: buffer.as_str().to_owned(),
            selected: buffer.selected_range().clone(),
            marked: buffer.marked_range().cloned(),
        }
    }

    fn selection_is_sound(shape: &Shape) -> bool {
        let Shape { text, selected, .. } = shape;
        selected.start <= selected.end
            && selected.end <= text.len()
            && text.is_char_boundary(selected.start)
            && text.is_char_boundary(selected.end)
    }

    struct CompositionCase {
        label: &'static str,
        seed: &'static str,
        caret: usize,
        updates: &'static [(&'static str, Option<Range<usize>>)],
        expected: (&'static str, Range<usize>, Option<Range<usize>>),
    }

    const COMPOSITIONS: &[CompositionCase] = &[
        CompositionCase {
            label: "kana replaces its latin pre-edit",
            seed: "",
            caret: 0,
            updates: &[("k", Some(1..1)), ("\u{304B}", Some(1..1))],
            expected: ("\u{304B}", 3..3, Some(0..3)),
        },
        CompositionCase {
            label: "multi-char composition with an astral emoji after a prefix",
            seed: "ab",
            caret: 2,
            updates: &[("n", Some(1..1)), ("\u{65E5}\u{1F600}", Some(1..3))],
            expected: ("ab\u{65E5}\u{1F600}", 5..9, Some(2..9)),
        },
        CompositionCase {
            label: "composition in the middle selects inside the composed text",
            seed: "ab",
            caret: 1,
            updates: &[("s", Some(1..1)), ("\u{3059}", Some(0..1))],
            expected: ("a\u{3059}b", 1..4, Some(1..4)),
        },
        CompositionCase {
            label: "selection past the composed text clamps to its end",
            seed: "",
            caret: 0,
            updates: &[("\u{304B}", Some(5..9))],
            expected: ("\u{304B}", 3..3, Some(0..3)),
        },
        CompositionCase {
            label: "reversed selection collapses at its start",
            seed: "",
            caret: 0,
            updates: &[("\u{65E5}\u{672C}", Some(Range { start: 2, end: 1 }))],
            expected: ("\u{65E5}\u{672C}", 6..6, Some(0..6)),
        },
        CompositionCase {
            label: "no selection leaves the caret after the composed text",
            seed: "xy",
            caret: 1,
            updates: &[("\u{304B}", None)],
            expected: ("x\u{304B}y", 4..4, Some(1..4)),
        },
    ];

    fn compose_each<T: Harness>(cx: &mut TestAppContext) -> Vec<(&'static str, Shape, bool)> {
        COMPOSITIONS
            .iter()
            .map(|case| {
                on::<T, _>(cx, case.seed, |editor, window, cx| {
                    editor.move_to(case.caret, cx);
                    let mut sound = true;
                    for (text, selected) in case.updates {
                        editor.replace_and_mark_text_in_range(
                            None,
                            text,
                            selected.clone(),
                            window,
                            cx,
                        );
                        sound &= selection_is_sound(&shape(editor));
                    }
                    (case.label, shape(editor), sound)
                })
            })
            .collect()
    }

    #[gpui::test]
    fn composition_selection_is_relative_to_the_composed_text_in_every_editor(
        cx: &mut TestAppContext,
    ) {
        let runs = [
            (TextInput::NAME, compose_each::<TextInput>(cx)),
            (TextArea::NAME, compose_each::<TextArea>(cx)),
            (CodeEditor::NAME, compose_each::<CodeEditor>(cx)),
        ];
        for (editor, outcomes) in runs {
            for (case, (label, got, _)) in COMPOSITIONS.iter().zip(outcomes) {
                let (text, selected, marked) = case.expected.clone();
                let want = Shape {
                    text: text.to_owned(),
                    selected,
                    marked,
                };
                assert_eq!(got, want, "{editor}: {label}");
            }
        }
    }

    #[gpui::test]
    fn every_composition_update_keeps_the_selection_inside_the_text_on_char_boundaries(
        cx: &mut TestAppContext,
    ) {
        let runs = [
            (TextInput::NAME, compose_each::<TextInput>(cx)),
            (TextArea::NAME, compose_each::<TextArea>(cx)),
            (CodeEditor::NAME, compose_each::<CodeEditor>(cx)),
        ];
        for (editor, outcomes) in runs {
            for (label, _, sound) in outcomes {
                assert!(sound, "{editor}: {label}");
            }
        }
    }

    fn type_after_unmarked_composition<T: Harness>(cx: &mut TestAppContext) -> Shape {
        on::<T, _>(cx, "", |editor, window, cx| {
            editor.replace_and_mark_text_in_range(None, "k", Some(1..1), window, cx);
            editor.replace_and_mark_text_in_range(None, "\u{304B}", Some(1..1), window, cx);
            editor.unmark_text(window, cx);
            editor.replace_text_in_range(None, "x", window, cx);
            shape(editor)
        })
    }

    #[gpui::test]
    fn typing_after_an_unmarked_composition_appends_after_the_whole_char(cx: &mut TestAppContext) {
        let want = Shape {
            text: "\u{304B}x".to_owned(),
            selected: 4..4,
            marked: None,
        };
        for (editor, got) in [
            (
                TextInput::NAME,
                type_after_unmarked_composition::<TextInput>(cx),
            ),
            (
                TextArea::NAME,
                type_after_unmarked_composition::<TextArea>(cx),
            ),
            (
                CodeEditor::NAME,
                type_after_unmarked_composition::<CodeEditor>(cx),
            ),
        ] {
            assert_eq!(got, want, "{editor}");
        }
    }

    type RangeRead = (Option<String>, Option<Range<usize>>);

    fn read_ranges<T: Harness>(cx: &mut TestAppContext) -> Vec<RangeRead> {
        on::<T, _>(cx, "a\u{1F600}b", |editor, window, cx| {
            [1..99, Range { start: 3, end: 1 }, 1..2, 7..9]
                .into_iter()
                .map(|range| {
                    let mut actual = None;
                    let text = editor.text_for_range(range, &mut actual, window, cx);
                    (text, actual)
                })
                .collect()
        })
    }

    #[gpui::test]
    fn text_for_range_clamps_out_of_range_and_refuses_reversed_ranges_in_every_editor(
        cx: &mut TestAppContext,
    ) {
        let want: Vec<RangeRead> = vec![
            (Some("\u{1F600}b".to_owned()), Some(1..4)),
            (None, Some(Range { start: 3, end: 1 })),
            (Some("\u{1F600}".to_owned()), Some(1..3)),
            (Some(String::new()), Some(4..4)),
        ];
        for (editor, got) in [
            (TextInput::NAME, read_ranges::<TextInput>(cx)),
            (TextArea::NAME, read_ranges::<TextArea>(cx)),
            (CodeEditor::NAME, read_ranges::<CodeEditor>(cx)),
        ] {
            assert_eq!(got, want, "{editor}");
        }
    }

    #[test]
    fn vertical_probe_lands_mid_row_and_stays_inside_the_content() {
        let line = px(20.0);
        for (caret_y, below, content_height, want) in [
            (px(0.0), false, px(100.0), px(0.0)),
            (px(40.0), false, px(100.0), px(30.0)),
            (px(40.0), true, px(100.0), px(70.0)),
            (px(60.0), true, px(100.0), px(90.0)),
            (px(80.0), true, px(100.0), px(99.0)),
            (px(0.0), true, px(20.0), px(19.0)),
            (px(0.0), true, px(0.0), px(0.0)),
            (px(0.0), true, px(0.5), px(0.0)),
        ] {
            assert_eq!(
                vertical_probe_y(caret_y, line, below, content_height),
                want,
                "caret {caret_y:?} below {below} height {content_height:?}"
            );
        }
    }
}
