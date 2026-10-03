mod element;
mod layout;
pub(crate) mod lines;

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, ScrollWheelEvent, SharedString, UTF16Selection, Window, actions, div, point,
    prelude::*, px,
};

use crate::caret_blink::{CaretBlink, CaretHost, set_caret_blinking};
use crate::highlight::Language;
use crate::palette::{FORGE_DEFAULT, ForgePalette};
use crate::text_area::{
    Backspace, Copy, Cut, Delete, Down, End, Home, InsertNewline, Left, Paste, Redo, Right,
    SelectAll, SelectDown, SelectLeft, SelectRight, SelectUp, Undo, Up, editing_key_bindings,
};
use crate::text_buffer::{EditKind, TextBuffer};
use crate::text_edit::{offset_to_utf16, range_from_utf16, range_to_utf16};
use crate::text_input::InputEvent;
use crate::tokens::{FONT_XS, mono_family};

use element::{CodeEditorElement, PAD_Y};
use layout::{Geometry, Lines};

const KEY_CONTEXT: &str = "ForgeCodeEditor";
const INDENT_UNIT: &str = "  ";
const LINE_HEIGHT_RATIO: f32 = 1.5;
const ROW_CENTER: f32 = 0.5;
const EDGE_INSET: Pixels = px(1.0);

actions!(forge_code_editor, [Indent, Outdent]);

pub fn bind_code_editor_keys(cx: &mut App) {
    let mut bindings = editing_key_bindings(KEY_CONTEXT);
    bindings.push(KeyBinding::new("tab", Indent, Some(KEY_CONTEXT)));
    bindings.push(KeyBinding::new("shift-tab", Outdent, Some(KEY_CONTEXT)));
    cx.bind_keys(bindings);
}

pub struct CodeEditor {
    focus_handle: FocusHandle,
    buffer: TextBuffer,
    lines: Lines,
    geometry: Geometry,
    placeholder: SharedString,
    palette: ForgePalette,
    font_size: Pixels,
    line_height: Pixels,
    gutter_marks: Vec<usize>,
    last_bounds: Option<Bounds<Pixels>>,
    text_left: Pixels,
    scroll_offset: Pixels,
    follow_caret: bool,
    is_selecting: bool,
    preferred_x: Option<Pixels>,
    caret: CaretBlink,
}

impl EventEmitter<InputEvent> for CodeEditor {}

impl CaretHost for CodeEditor {
    fn caret(&mut self) -> &mut CaretBlink {
        &mut self.caret
    }
}

impl CodeEditor {
    pub fn new(
        language: Language,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            buffer: TextBuffer::default(),
            lines: Lines::new(language),
            geometry: Geometry::default(),
            placeholder: placeholder.into(),
            palette: FORGE_DEFAULT,
            font_size: FONT_XS,
            line_height: FONT_XS * LINE_HEIGHT_RATIO,
            gutter_marks: Vec::new(),
            last_bounds: None,
            text_left: px(0.0),
            scroll_offset: px(0.0),
            follow_caret: false,
            is_selecting: false,
            preferred_x: None,
            caret: CaretBlink::new(),
        }
    }

    pub fn with_palette(mut self, palette: ForgePalette) -> Self {
        self.palette = palette;
        self
    }

    pub fn with_font_size(mut self, size: Pixels) -> Self {
        self.font_size = size;
        self.line_height = size * LINE_HEIGHT_RATIO;
        self
    }

    pub fn with_line_height(mut self, line_height: Pixels) -> Self {
        self.line_height = line_height;
        self
    }

    pub fn content(&self) -> &str {
        self.buffer.as_str()
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn language(&self) -> Language {
        self.lines.language()
    }

    pub fn set_content(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.buffer.reset(text.into().into());
        self.buffer.move_to(0);
        self.lines.sync(self.buffer.text());
        self.geometry.invalidate();
        self.preferred_x = None;
        self.scroll_offset = px(0.0);
        self.follow_caret = false;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set_content("", cx);
    }

    pub fn set_language(&mut self, language: Language, cx: &mut Context<Self>) {
        if self.lines.set_language(language) {
            self.geometry.invalidate();
            cx.notify();
        }
    }

    pub fn set_palette(&mut self, palette: ForgePalette, cx: &mut Context<Self>) {
        self.palette = palette;
        self.lines.reset_payloads();
        self.geometry.invalidate();
        cx.notify();
    }

    pub fn set_gutter_marks(&mut self, lines: Vec<usize>, cx: &mut Context<Self>) {
        self.gutter_marks = lines;
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.buffer.move_left();
        self.caret_moved(cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.buffer.move_right();
        self.caret_moved(cx);
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(false, false, window, cx);
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(true, false, window, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.buffer.select_left();
        self.caret_moved(cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.buffer.select_right();
        self.caret_moved(cx);
    }

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(false, true, window, cx);
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(true, true, window, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.buffer.select_all();
        self.caret_moved(cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.buffer.move_to_line_start();
        self.caret_moved(cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.buffer.move_to_line_end();
        self.caret_moved(cx);
    }

    fn insert_newline(&mut self, _: &InsertNewline, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.buffer.insert_newline_keeping_indent();
        self.edited(cx);
    }

    fn indent(&mut self, _: &Indent, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.buffer.indent(INDENT_UNIT);
        self.edited(cx);
    }

    fn outdent(&mut self, _: &Outdent, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.buffer.outdent(INDENT_UNIT);
        self.edited(cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.buffer.delete_backward();
        self.edited(cx);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.buffer.delete_forward();
        self.edited(cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.buffer.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.buffer.selected_text() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        self.preferred_x = None;
        self.buffer.insert("", EditKind::Standalone);
        self.edited(cx);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.preferred_x = None;
            self.buffer.insert(&text, EditKind::Standalone);
            self.edited(cx);
        }
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.undo() {
            self.preferred_x = None;
            self.edited(cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.redo() {
            self.preferred_x = None;
            self.edited(cx);
        }
    }

    fn caret_moved(&mut self, cx: &mut Context<Self>) {
        self.follow_caret = true;
        self.caret.wake();
        cx.notify();
    }

    fn edited(&mut self, cx: &mut Context<Self>) {
        if self.lines.sync(self.buffer.text()) {
            self.geometry.invalidate();
        }
        self.follow_caret = true;
        self.caret.wake();
        cx.emit(InputEvent::Changed(self.buffer.text().into()));
        cx.notify();
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
        self.preferred_x = None;
        let offset = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.buffer.select_to(offset);
        } else {
            self.buffer.move_to(offset);
        }
        self.caret_moved(cx);
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.buffer
                .select_to(self.index_for_mouse_position(event.position));
            self.caret_moved(cx);
        }
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(self.line_height).y;
        if delta == px(0.0) {
            return;
        }
        let next = (self.scroll_offset - delta).clamp(px(0.0), self.max_scroll());
        if next != self.scroll_offset {
            self.scroll_offset = next;
            self.follow_caret = false;
            cx.notify();
        }
    }

    fn max_scroll(&self) -> Pixels {
        let view_height = self
            .last_bounds
            .map_or(px(0.0), |bounds| bounds.size.height);
        (self.geometry.total() + PAD_Y + PAD_Y - view_height).max(px(0.0))
    }

    fn shape_around(&mut self, index: usize, window: &mut Window) {
        let first = index.saturating_sub(1);
        let last = (index + 1).min(self.lines.len().saturating_sub(1));
        for line in first..=last {
            self.geometry
                .shape_line(&mut self.lines, line, &self.palette, window);
        }
        self.geometry.refresh(&self.lines);
    }

    fn move_vertical(
        &mut self,
        down: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cursor = self.buffer.cursor();
        self.shape_around(self.lines.line_index_at(cursor), window);
        let Some(line_height) = self.geometry.metrics().map(|metrics| metrics.line_height) else {
            return;
        };
        let Some(caret) = self.geometry.point_for_offset(&self.lines, cursor) else {
            return;
        };
        let goal_x = self.preferred_x.unwrap_or(caret.x);
        let target_y = if down {
            caret.y + line_height + line_height * ROW_CENTER
        } else {
            caret.y - line_height * ROW_CENTER
        };
        let last_y = (self.geometry.total() - EDGE_INSET).max(px(0.0));
        let target = self
            .geometry
            .offset_for_point(&self.lines, point(goal_x, target_y.clamp(px(0.0), last_y)));
        self.preferred_x = Some(goal_x);
        if extend {
            self.buffer.select_to(target);
        } else {
            self.buffer.move_to(target);
        }
        self.caret_moved(cx);
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        let x = position.x - bounds.left() - self.text_left;
        let y = (position.y - bounds.top() - PAD_Y + self.scroll_offset).max(px(0.0));
        self.geometry.offset_for_point(&self.lines, point(x, y))
    }
}

impl EntityInputHandler for CodeEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let content = self.buffer.as_str();
        let range = range_from_utf16(content, &range_utf16);
        actual_range.replace(range_to_utf16(content, &range));
        content.get(range).map(str::to_string)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: range_to_utf16(self.buffer.as_str(), self.buffer.selected_range()),
            reversed: self.buffer.is_reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.buffer
            .marked_range()
            .map(|range| range_to_utf16(self.buffer.as_str(), range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.buffer.unmark();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.replace_in_utf16_range(range_utf16, new_text);
        self.edited(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.replace_and_mark_in_utf16_range(
            range_utf16,
            new_text,
            new_selected_range_utf16,
        );
        self.edited(cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line_height = self.geometry.metrics()?.line_height;
        let range = range_from_utf16(self.buffer.as_str(), &range_utf16);
        let start = self.geometry.point_for_offset(&self.lines, range.start)?;
        let origin = point(
            bounds.left() + self.text_left + start.x,
            bounds.top() + PAD_Y + start.y - self.scroll_offset,
        );
        Some(Bounds::from_corners(
            origin,
            point(origin.x, origin.y + line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.index_for_mouse_position(point);
        Some(offset_to_utf16(self.buffer.as_str(), offset))
    }
}

impl Focusable for CodeEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CodeEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus_handle.clone();
        self.caret.watch(&focus, window, cx);
        set_caret_blinking(self, focus.is_focused(window) && window.is_visible(), cx);

        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::insert_newline))
            .on_action(cx.listener(Self::indent))
            .on_action(cx.listener(Self::outdent))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .overflow_hidden()
            .bg(self.palette.base)
            .font_family(mono_family())
            .text_size(self.font_size)
            .text_color(self.palette.text_primary)
            .line_height(self.line_height)
            .child(CodeEditorElement::new(cx.entity()))
    }
}
