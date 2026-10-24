use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EventEmitter, FocusHandle, Focusable, GlobalElementId, Hsla, LayoutId, PaintQuad,
    Pixels, Point, ScrollWheelEvent, SharedString, Style, TextAlign, TextRun, UnderlineStyle,
    Window, WrappedLine, div, fill, point, prelude::*, px, relative, size,
};

use crate::caret_blink::{CaretBlink, CaretHost, set_caret_blinking};
use crate::palette::{FORGE_DEFAULT, ForgePalette, with_alpha};
use crate::text_buffer::{EditKind, TextBuffer};
use crate::text_editing::*;
use crate::text_input::InputEvent;
use crate::tokens::{
    BORDER_THIN, Density, FONT_XS, Radius, Spacing, body_family, mono_family, radius, spacing,
};

const KEY_CONTEXT: &str = "ForgeTextArea";

const DEFAULT_AREA_HEIGHT: Pixels = px(130.0);

pub fn bind_text_area_keys(cx: &mut App) {
    cx.bind_keys(editing_key_bindings(KEY_CONTEXT));
}

struct AreaLayout {
    lines: Vec<WrappedLine>,
    para_tops: Vec<Pixels>,
    para_byte_starts: Vec<usize>,
    line_height: Pixels,
    total_height: Pixels,
}

impl AreaLayout {
    fn point_for_offset(&self, offset: usize) -> Option<Point<Pixels>> {
        for i in (0..self.lines.len()).rev() {
            let start = self.para_byte_starts[i];
            if offset >= start {
                let local = (offset - start).min(self.lines[i].len());
                let inner = self.lines[i].position_for_index(local, self.line_height)?;
                return Some(point(inner.x, self.para_tops[i] + inner.y));
            }
        }
        None
    }

    fn offset_for_point(&self, p: Point<Pixels>) -> usize {
        for i in (0..self.lines.len()).rev() {
            if p.y >= self.para_tops[i] {
                let local_point = point(p.x, p.y - self.para_tops[i]);
                let idx = self.lines[i]
                    .closest_index_for_position(local_point, self.line_height)
                    .unwrap_or_else(|clamped| clamped);
                return self.para_byte_starts[i] + idx.min(self.lines[i].len());
            }
        }
        0
    }
}

pub struct TextArea {
    focus_handle: FocusHandle,
    buffer: TextBuffer,
    placeholder: SharedString,
    last_layout: Option<AreaLayout>,
    last_bounds: Option<Bounds<Pixels>>,
    scroll_offset: Pixels,
    motion: CaretMotion,
    palette: ForgePalette,
    density: Density,
    font_size: Pixels,
    font_family: SharedString,
    height: Pixels,
    on_surface: bool,
    follow_caret: bool,
    caret: CaretBlink,
    invalid: bool,
}

impl EventEmitter<InputEvent> for TextArea {}

impl CaretHost for TextArea {
    fn caret(&mut self) -> &mut CaretBlink {
        &mut self.caret
    }
}

impl TextArea {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            buffer: TextBuffer::default(),
            placeholder: placeholder.into(),
            last_layout: None,
            last_bounds: None,
            scroll_offset: px(0.0),
            motion: CaretMotion::default(),
            palette: FORGE_DEFAULT,
            density: Density::Cozy,
            font_size: FONT_XS,
            font_family: body_family(),
            height: DEFAULT_AREA_HEIGHT,
            on_surface: false,
            follow_caret: true,
            caret: CaretBlink::new(),
            invalid: false,
        }
    }

    pub fn with_palette(mut self, palette: ForgePalette) -> Self {
        self.palette = palette;
        self
    }

    pub fn with_font_size(mut self, size: Pixels) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    pub fn mono(mut self) -> Self {
        self.font_family = mono_family();
        self
    }

    pub fn on_surface(mut self) -> Self {
        self.on_surface = true;
        self
    }

    pub fn content(&self) -> &str {
        self.buffer.as_str()
    }

    pub fn set_palette(&mut self, palette: ForgePalette, cx: &mut Context<Self>) {
        self.palette = palette;
        cx.notify();
    }

    pub fn set_height(&mut self, height: Pixels, cx: &mut Context<Self>) {
        self.height = height;
        cx.notify();
    }

    pub fn set_invalid(&mut self, invalid: bool, cx: &mut Context<Self>) {
        if self.invalid != invalid {
            self.invalid = invalid;
            cx.notify();
        }
    }

    pub fn set_content(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.buffer.reset(text.into().into());
        self.motion.goal_x = None;
        self.scroll_offset = px(0.0);
        self.follow_caret = false;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set_content("", cx);
    }

    pub fn insert_at_caret(&mut self, text: &str, cx: &mut Context<Self>) {
        self.insert_text(text, cx);
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    fn insert_newline(&mut self, _: &InsertNewline, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(|buffer| buffer.insert("\n", EditKind::Standalone), cx);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx
            .read_from_clipboard()
            .as_ref()
            .and_then(ClipboardItem::text)
        {
            self.insert_text(&text, cx);
        }
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line_height = self.font_size * 1.5;
        let delta = event.delta.pixel_delta(line_height).y;
        if delta == px(0.0) {
            return;
        }
        let view_h = self.last_bounds.map_or(px(0.0), |b| b.size.height);
        let total_h = self
            .last_layout
            .as_ref()
            .map_or(px(0.0), |l| l.total_height);
        let max_scroll = (total_h - view_h).max(px(0.0));
        let next = (self.scroll_offset - delta).clamp(px(0.0), max_scroll);
        if next != self.scroll_offset {
            self.scroll_offset = next;
            self.follow_caret = false;
            cx.notify();
        }
    }
}

impl TextEditor for TextArea {
    fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    fn buffer_mut(&mut self) -> &mut TextBuffer {
        &mut self.buffer
    }

    fn motion(&mut self) -> &mut CaretMotion {
        &mut self.motion
    }

    fn reveal_caret(&mut self) {
        self.follow_caret = true;
    }

    fn offset_for_position(&self, position: Point<Pixels>) -> usize {
        if self.buffer.as_str().is_empty() {
            return 0;
        }
        let (Some(bounds), Some(layout)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        let x = position.x - bounds.left();
        let mut y = position.y - bounds.top() + self.scroll_offset;
        if y < px(0.0) {
            y = px(0.0);
        }
        let max_y = layout.total_height - px(1.0);
        if max_y > px(0.0) && y > max_y {
            y = max_y;
        }
        layout.offset_for_point(point(x, y))
    }

    fn bounds_for_offsets(
        &self,
        range: Range<usize>,
        element_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let start = layout.point_for_offset(range.start)?;
        let top = element_bounds.top() + start.y - self.scroll_offset;
        let x = element_bounds.left() + start.x;
        Some(Bounds::from_corners(
            point(x, top),
            point(x, top + layout.line_height),
        ))
    }

    fn vertical_target(
        &mut self,
        below: bool,
        goal_x: Option<Pixels>,
        _: &mut Window,
    ) -> Option<(Pixels, usize)> {
        let layout = self.last_layout.as_ref()?;
        let caret = layout.point_for_offset(self.buffer.cursor())?;
        let goal_x = goal_x.unwrap_or(caret.x);
        let probe_y = vertical_probe_y(caret.y, layout.line_height, below, layout.total_height);
        Some((goal_x, layout.offset_for_point(point(goal_x, probe_y))))
    }
}

forward_input_handler!(TextArea);

fn apply_range(runs: &mut Vec<TextRun>, start: usize, end: usize, f: impl Fn(&mut TextRun)) {
    let mut out: Vec<TextRun> = Vec::with_capacity(runs.len() + 2);
    let mut pos = 0usize;
    for run in runs.drain(..) {
        let run_start = pos;
        let run_end = pos + run.len;
        pos = run_end;
        if run_end <= start || run_start >= end || run.len == 0 {
            out.push(run);
            continue;
        }
        let a = start.max(run_start);
        let b = end.min(run_end);
        if a > run_start {
            out.push(TextRun {
                len: a - run_start,
                ..run.clone()
            });
        }
        let mut mid = TextRun {
            len: b - a,
            ..run.clone()
        };
        f(&mut mid);
        out.push(mid);
        if run_end > b {
            out.push(TextRun {
                len: run_end - b,
                ..run
            });
        }
    }
    *runs = out;
}

fn build_runs(
    text: &SharedString,
    base_color: Hsla,
    font: gpui::Font,
    selection: &Range<usize>,
    selection_bg: Hsla,
    marked_range: Option<&Range<usize>>,
) -> Vec<TextRun> {
    let mut runs = vec![TextRun {
        len: text.len(),
        font,
        color: base_color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }];

    if let Some(marked) = marked_range {
        apply_range(&mut runs, marked.start, marked.end, |run| {
            run.underline = Some(UnderlineStyle {
                color: Some(run.color),
                thickness: px(1.0),
                wavy: false,
            });
        });
    } else if !selection.is_empty() && selection.end <= text.len() {
        apply_range(&mut runs, selection.start, selection.end, |run| {
            run.background_color = Some(selection_bg);
        });
    }

    runs.into_iter().filter(|run| run.len > 0).collect()
}

struct AreaElement {
    input: Entity<TextArea>,
}

struct PrepaintState {
    layout: Option<AreaLayout>,
    cursor: Option<PaintQuad>,
    scroll_offset: Pixels,
}

impl IntoElement for AreaElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for AreaElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = SharedString::from(input.buffer.text());
        let selected_range = input.buffer.selected_range().clone();
        let marked_range = input.buffer.marked_range().cloned();
        let cursor = input.cursor_offset();
        let palette = input.palette;
        let style = window.text_style();
        let line_height = window.line_height();

        let (display_text, base_color): (SharedString, Hsla) = if content.is_empty() {
            (input.placeholder.clone(), palette.text_muted.into())
        } else {
            (content.clone(), style.color)
        };

        let selection = if content.is_empty() {
            0..0
        } else {
            selected_range.clone()
        };
        let runs = build_runs(
            &display_text,
            base_color,
            style.font(),
            &selection,
            with_alpha(palette.brand, 0.25).into(),
            marked_range.as_ref(),
        );

        let font_size = style.font_size.to_pixels(window.rem_size());
        let wrap_width = if bounds.size.width > px(0.0) {
            Some(bounds.size.width)
        } else {
            None
        };
        let shaped = window
            .text_system()
            .shape_text(display_text, font_size, &runs, wrap_width, None)
            .unwrap_or_default();

        let mut para_tops = Vec::with_capacity(shaped.len());
        let mut para_byte_starts = Vec::with_capacity(shaped.len());
        let mut y = px(0.0);
        let mut byte = 0usize;
        for line in shaped.iter() {
            para_tops.push(y);
            para_byte_starts.push(byte);
            y += line.size(line_height).height;
            byte += line.len() + 1; // +1 for the '\n' the shaper consumed between paragraphs
        }
        let layout = AreaLayout {
            lines: shaped.into_iter().collect(),
            para_tops,
            para_byte_starts,
            line_height,
            total_height: y,
        };

        let mut scroll_offset = input.scroll_offset;
        let view_h = bounds.size.height;
        let caret_point = if selected_range.is_empty() {
            layout.point_for_offset(cursor)
        } else {
            None
        };
        if input.follow_caret
            && let Some(caret) = caret_point
        {
            if caret.y - scroll_offset < px(0.0) {
                scroll_offset = caret.y;
            }
            if caret.y + line_height - scroll_offset > view_h {
                scroll_offset = caret.y + line_height - view_h;
            }
        }
        if scroll_offset < px(0.0) {
            scroll_offset = px(0.0);
        }
        let max_scroll = layout.total_height - view_h;
        if max_scroll > px(0.0) {
            if scroll_offset > max_scroll {
                scroll_offset = max_scroll;
            }
        } else {
            scroll_offset = px(0.0);
        }

        let cursor = caret_point.map(|caret| {
            fill(
                Bounds::new(
                    point(
                        bounds.left() + caret.x,
                        bounds.top() + caret.y - scroll_offset,
                    ),
                    size(px(1.5), line_height),
                ),
                palette.text_primary,
            )
        });

        PrepaintState {
            layout: Some(layout),
            cursor,
            scroll_offset,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        let scroll_offset = prepaint.scroll_offset;
        let Some(layout) = prepaint.layout.take() else {
            return;
        };

        for (i, line) in layout.lines.iter().enumerate() {
            let origin = point(
                bounds.left(),
                bounds.top() + layout.para_tops[i] - scroll_offset,
            );
            let _ = line.paint_background(
                origin,
                layout.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
            let _ = line.paint(
                origin,
                layout.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }

        if focus_handle.is_focused(window)
            && self.input.read(cx).caret.is_lit()
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(layout);
            input.last_bounds = Some(bounds);
            input.scroll_offset = scroll_offset;
        });
    }
}

impl Focusable for TextArea {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextArea {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus_handle.clone();
        self.caret.watch(&focus, window, cx);
        let focused = focus.is_focused(window);
        set_caret_blinking(self, focused && window.is_visible(), cx);
        let border_color = if focused {
            self.palette.border_active
        } else {
            self.palette.border_input
        };
        let border_color = if self.invalid {
            self.palette.random
        } else {
            border_color
        };
        let surface = if self.on_surface {
            self.palette.elevated
        } else {
            self.palette.shell
        };

        let field = div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::insert_newline))
            .on_action(cx.listener(Self::paste))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel));
        let field = with_multiline_actions(with_editing_actions(field, cx), cx)
            .w_full()
            .overflow_hidden()
            .font_family(self.font_family.clone())
            .text_size(self.font_size)
            .text_color(self.palette.text_primary)
            .line_height(self.font_size * 1.5)
            .h(self.height)
            .px(spacing(Spacing::Sm, self.density))
            .py(spacing(Spacing::Xs, self.density))
            .bg(surface)
            .border(BORDER_THIN)
            .border_color(border_color)
            .rounded(radius(Radius::Md));
        field.child(AreaElement { input: cx.entity() })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn with_area<R>(
        cx: &mut gpui::TestAppContext,
        content: &str,
        f: impl FnOnce(&mut TextArea, &mut Window, &mut Context<TextArea>) -> R,
    ) -> R {
        let window = cx.add_window(|_window, cx| TextArea::new("placeholder", cx));
        window
            .update(cx, |area, window, cx| {
                area.set_content(content.to_string(), cx);
                f(area, window, cx)
            })
            .unwrap()
    }

    fn with_painted_area<R>(
        cx: &mut gpui::TestAppContext,
        content: &str,
        f: impl FnOnce(&mut TextArea, &mut Window, &mut Context<TextArea>) -> R,
    ) -> R {
        let window = cx.add_window(|_window, cx| TextArea::new("placeholder", cx));
        window
            .update(cx, |area, _window, cx| {
                area.set_content(content.to_string(), cx);
            })
            .unwrap();
        cx.run_until_parked();
        window
            .update(cx, |area, window, cx| f(area, window, cx))
            .unwrap()
    }

    #[gpui::test]
    fn enter_inserts_a_newline_at_the_cursor_instead_of_submitting(cx: &mut gpui::TestAppContext) {
        let (content, cursor) = with_area(cx, "abcd", |area, window, cx| {
            area.move_to(2, cx);
            area.insert_newline(&InsertNewline, window, cx);
            (area.content().to_string(), area.cursor_offset())
        });
        assert_eq!(content, "ab\ncd");
        assert_eq!(cursor, 3);
    }

    #[gpui::test]
    fn down_holds_the_goal_column_across_a_shorter_intervening_line(cx: &mut gpui::TestAppContext) {
        let stops = with_painted_area(cx, "long line\nx\nanother line", |area, window, cx| {
            area.move_to(6, cx);
            let mut seen = Vec::new();
            area.down(&Down, window, cx);
            seen.push(area.cursor_offset());
            area.down(&Down, window, cx);
            seen.push(area.cursor_offset());
            seen
        });
        assert_eq!(stops, vec![11usize, 18]);
    }

    #[gpui::test]
    fn up_holds_the_goal_column_across_a_shorter_intervening_line(cx: &mut gpui::TestAppContext) {
        let stops = with_painted_area(cx, "long line\nx\nanother line", |area, window, cx| {
            area.move_to(18, cx);
            let mut seen = Vec::new();
            area.up(&Up, window, cx);
            seen.push(area.cursor_offset());
            area.up(&Up, window, cx);
            seen.push(area.cursor_offset());
            seen
        });
        assert_eq!(stops, vec![11usize, 6]);
    }

    #[gpui::test]
    fn paste_preserves_embedded_newlines(cx: &mut gpui::TestAppContext) {
        let (content, cursor) = with_area(cx, "", |area, window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("a\nb\nc".to_string()));
            area.paste(&Paste, window, cx);
            (area.content().to_string(), area.cursor_offset())
        });
        assert_eq!(content, "a\nb\nc");
        assert_eq!(cursor, 5);
    }

    #[gpui::test]
    fn home_moves_to_the_start_of_the_current_hard_line(cx: &mut gpui::TestAppContext) {
        let cursor = with_area(cx, "first\nsecond\nthird", |area, window, cx| {
            area.move_to(9, cx);
            area.home(&Home, window, cx);
            area.cursor_offset()
        });
        assert_eq!(cursor, 6);
    }

    #[gpui::test]
    fn end_moves_to_the_end_of_the_current_hard_line(cx: &mut gpui::TestAppContext) {
        let cursor = with_area(cx, "first\nsecond\nthird", |area, window, cx| {
            area.move_to(9, cx);
            area.end(&End, window, cx);
            area.cursor_offset()
        });
        assert_eq!(cursor, 12);
    }

    fn changes_after(
        cx: &mut gpui::TestAppContext,
        f: impl FnOnce(&mut TextArea, &mut Window, &mut Context<TextArea>),
    ) -> (String, Vec<String>) {
        let window = cx.add_window(|_window, cx| TextArea::new("placeholder", cx));
        let area = window.root(cx).unwrap();
        window
            .update(cx, |area, window, cx| {
                area.replace_text_in_range(None, "ab", window, cx);
            })
            .unwrap();
        let heard = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = std::rc::Rc::clone(&heard);
        let _subscription = cx.update(|cx| {
            cx.subscribe(&area, move |_, event: &InputEvent, _| {
                if let InputEvent::Changed(text) = event {
                    sink.borrow_mut().push(text.to_string());
                }
            })
        });
        let content = window
            .update(cx, |area, window, cx| {
                f(area, window, cx);
                area.content().to_string()
            })
            .unwrap();
        let changes = heard.borrow().clone();
        (content, changes)
    }

    #[gpui::test]
    fn undo_and_redo_announce_the_restored_content(cx: &mut gpui::TestAppContext) {
        let (_, changes) = changes_after(cx, |area, window, cx| {
            area.undo(&Undo, window, cx);
            area.redo(&Redo, window, cx);
        });
        assert_eq!(changes, vec!["".to_string(), "ab".to_string()]);
    }

    #[gpui::test]
    fn undo_with_nothing_to_undo_announces_no_change(cx: &mut gpui::TestAppContext) {
        let (_, changes) = changes_after(cx, |area, window, cx| {
            area.undo(&Undo, window, cx);
            area.undo(&Undo, window, cx);
            area.redo(&Redo, window, cx);
            area.redo(&Redo, window, cx);
        });
        assert_eq!(changes, vec!["".to_string(), "ab".to_string()]);
    }

    #[gpui::test]
    fn insert_at_caret_lands_at_the_cursor_and_advances_it(cx: &mut gpui::TestAppContext) {
        let (content, cursor) = with_area(cx, "ab\ncd", |area, _window, cx| {
            area.move_to(4, cx);
            area.insert_at_caret("%x%", cx);
            (area.content().to_string(), area.cursor_offset())
        });
        assert_eq!((content.as_str(), cursor), ("ab\nc%x%d", 7));
    }

    #[gpui::test]
    fn insert_at_caret_replaces_the_selection(cx: &mut gpui::TestAppContext) {
        let content = with_area(cx, "hello", |area, _window, cx| {
            area.move_to(1, cx);
            area.select_to(4, cx);
            area.insert_at_caret("%x%", cx);
            area.content().to_string()
        });
        assert_eq!(content, "h%x%o");
    }

    #[gpui::test]
    fn an_inserted_token_is_one_undo_step_apart_from_typing(cx: &mut gpui::TestAppContext) {
        let (_, changes) = changes_after(cx, |area, window, cx| {
            area.insert_at_caret("%x%", cx);
            area.undo(&Undo, window, cx);
            area.redo(&Redo, window, cx);
        });
        assert_eq!(changes, vec!["ab%x%", "ab", "ab%x%"]);
    }

    fn tall_content() -> String {
        (0..200)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[gpui::test]
    fn set_content_shows_the_top_with_the_caret_at_the_end(cx: &mut gpui::TestAppContext) {
        let content = tall_content();
        let (scroll, cursor) = with_painted_area(cx, &content, |area, _window, _cx| {
            (area.scroll_offset, area.cursor_offset())
        });
        assert_eq!((scroll, cursor), (px(0.0), content.len()));
    }

    #[gpui::test]
    fn set_content_on_a_scrolled_area_returns_to_the_top(cx: &mut gpui::TestAppContext) {
        let content = tall_content();
        let window = cx.add_window(|_window, cx| TextArea::new("placeholder", cx));
        window
            .update(cx, |area, _window, cx| {
                area.set_content(content.clone(), cx);
                area.insert_at_caret("x", cx);
            })
            .unwrap();
        cx.run_until_parked();
        let scrolled = window.update(cx, |area, _, _| area.scroll_offset).unwrap();
        window
            .update(cx, |area, _, cx| area.set_content(content.clone(), cx))
            .unwrap();
        cx.run_until_parked();
        let after = window.update(cx, |area, _, _| area.scroll_offset).unwrap();
        assert!(
            scrolled > px(0.0) && after == px(0.0),
            "{scrolled:?} -> {after:?}"
        );
    }
}
