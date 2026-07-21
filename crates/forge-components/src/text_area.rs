use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId,
    HighlightStyle, Hsla, KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, Pixels, Point, ScrollWheelEvent, SharedString, Style, StyledText,
    TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window, WrappedLine, actions, div, fill,
    point, prelude::*, px, relative, size,
};

use crate::palette::{FORGE_DEFAULT, ForgePalette, with_alpha};
use crate::text_edit::{
    next_grapheme_boundary, offset_to_utf16, previous_grapheme_boundary, range_from_utf16,
    range_to_utf16,
};
use crate::text_input::InputEvent;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_XS, Radius, Spacing,
    radius, spacing,
};

const KEY_CONTEXT: &str = "ForgeTextArea";

const CARET_BLINK_MS: u64 = 530;

fn spawn_caret_blink(cx: &mut Context<TextArea>) {
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(CARET_BLINK_MS))
                .await;
            let alive = this
                .update(cx, |this, cx| {
                    if this.focused_cached {
                        this.blink_visible = !this.blink_visible;
                        cx.notify();
                    } else {
                        this.blink_visible = true;
                    }
                })
                .is_ok();
            if !alive {
                break;
            }
        }
    })
    .detach();
}

const DEFAULT_AREA_HEIGHT: Pixels = px(130.0);

const GUTTER_W: Pixels = px(48.0);
const GUTTER_PAD_R: Pixels = px(8.0);
const GUTTER_ACCENT_W: Pixels = px(2.0);
const GUTTER_MARK: &str = "\u{25cf}";

actions!(
    forge_text_area,
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
    ]
);

/// The binary MUST call this once at boot or navigation, newline and editing keys are dead.
pub fn bind_text_area_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some(KEY_CONTEXT)),
        KeyBinding::new("delete", Delete, Some(KEY_CONTEXT)),
        KeyBinding::new("left", Left, Some(KEY_CONTEXT)),
        KeyBinding::new("right", Right, Some(KEY_CONTEXT)),
        KeyBinding::new("up", Up, Some(KEY_CONTEXT)),
        KeyBinding::new("down", Down, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-up", SelectUp, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-down", SelectDown, Some(KEY_CONTEXT)),
        KeyBinding::new("home", Home, Some(KEY_CONTEXT)),
        KeyBinding::new("end", End, Some(KEY_CONTEXT)),
        KeyBinding::new("enter", InsertNewline, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-a", SelectAll, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-a", SelectAll, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-c", Copy, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-c", Copy, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-x", Cut, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-x", Cut, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-v", Paste, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-v", Paste, Some(KEY_CONTEXT)),
    ]);
}

struct AreaLayout {
    lines: Vec<WrappedLine>,
    para_tops: Vec<Pixels>,
    para_byte_starts: Vec<usize>,
    line_height: Pixels,
    total_height: Pixels,
}

impl AreaLayout {
    /// Content-space, pre-scroll: the caller adds the viewport origin and subtracts the scroll offset.
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

    /// `p` is in content space, pre-scroll; clamps to the nearest paragraph and grapheme boundary.
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

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SyntaxMode {
    #[default]
    None,
    Json,
    Rhai,
}

/// Subscribe to [`InputEvent`] for edits - only `Changed` is emitted (a text area has no submit).
pub struct TextArea {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<AreaLayout>,
    last_bounds: Option<Bounds<Pixels>>,
    scroll_offset: Pixels,
    is_selecting: bool,
    /// Goal column held across a run of Up/Down moves; cleared by any horizontal move or edit.
    preferred_x: Option<Pixels>,
    palette: ForgePalette,
    density: Density,
    font_size: Pixels,
    font_family: &'static str,
    read_only: bool,
    height: Pixels,
    on_surface: bool,
    syntax: SyntaxMode,
    gutter: bool,
    gutter_marks: Vec<usize>,
    fill: bool,
    /// When true, prepaint keeps the caret in view (after edits/moves); a wheel
    /// scroll clears it so the content can be scrolled away from the caret.
    follow_caret: bool,
    blink_visible: bool,
    focused_cached: bool,
}

impl EventEmitter<InputEvent> for TextArea {}

impl TextArea {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        spawn_caret_blink(cx);
        Self {
            focus_handle: cx.focus_handle(),
            content: SharedString::default(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            scroll_offset: px(0.0),
            is_selecting: false,
            preferred_x: None,
            palette: FORGE_DEFAULT,
            density: Density::Cozy,
            font_size: FONT_XS,
            font_family: DEFAULT_BODY_FAMILY,
            read_only: false,
            height: DEFAULT_AREA_HEIGHT,
            on_surface: false,
            syntax: SyntaxMode::None,
            gutter: false,
            gutter_marks: Vec::new(),
            fill: false,
            follow_caret: true,
            blink_visible: true,
            focused_cached: false,
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
        self.font_family = DEFAULT_MONO_FAMILY;
        self
    }

    pub fn json_highlight(mut self) -> Self {
        self.syntax = SyntaxMode::Json;
        self
    }

    pub fn rhai_highlight(mut self) -> Self {
        self.syntax = SyntaxMode::Rhai;
        self
    }

    pub fn on_surface(mut self) -> Self {
        self.on_surface = true;
        self
    }

    pub fn with_gutter(mut self) -> Self {
        self.gutter = true;
        self
    }

    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    pub fn set_gutter_marks(&mut self, lines: Vec<usize>, cx: &mut Context<Self>) {
        self.gutter_marks = lines;
        cx.notify();
    }

    fn gutter_width(&self) -> Pixels {
        if self.gutter { GUTTER_W } else { px(0.0) }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn set_palette(&mut self, palette: ForgePalette, cx: &mut Context<Self>) {
        self.palette = palette;
        cx.notify();
    }

    pub fn set_height(&mut self, height: Pixels, cx: &mut Context<Self>) {
        self.height = height;
        cx.notify();
    }

    pub fn set_content(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = text.into();
        let end = self.content.len();
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
        self.preferred_x = None;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set_content("", cx);
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.selected_range.is_empty() {
            self.move_to(
                previous_grapheme_boundary(&self.content, self.cursor_offset()),
                cx,
            );
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.selected_range.is_empty() {
            self.move_to(
                next_grapheme_boundary(&self.content, self.selected_range.end),
                cx,
            );
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(false, false, cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(true, false, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.select_to(
            previous_grapheme_boundary(&self.content, self.cursor_offset()),
            cx,
        );
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.select_to(
            next_grapheme_boundary(&self.content, self.cursor_offset()),
            cx,
        );
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(false, true, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(true, true, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        let (start, _) = self.current_line_bounds();
        self.move_to(start, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        let (_, end) = self.current_line_bounds();
        self.move_to(end, cx);
    }

    fn insert_newline(&mut self, _: &InsertNewline, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        self.preferred_x = None;
        self.replace_text_in_range(None, "\n", window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        self.preferred_x = None;
        if self.selected_range.is_empty() {
            self.select_to(
                previous_grapheme_boundary(&self.content, self.cursor_offset()),
                cx,
            );
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        self.preferred_x = None;
        if self.selected_range.is_empty() {
            self.select_to(
                next_grapheme_boundary(&self.content, self.cursor_offset()),
                cx,
            );
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only || self.selected_range.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.content[self.selected_range.clone()].to_string(),
        ));
        self.preferred_x = None;
        self.replace_text_in_range(None, "", window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        // Newlines are preserved here, unlike the single-line input.
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.preferred_x = None;
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
        self.preferred_x = None;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
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

    fn current_line_bounds(&self) -> (usize, usize) {
        let cursor = self.cursor_offset();
        let start = self.content[..cursor]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = self.content[cursor..]
            .find('\n')
            .map(|i| cursor + i)
            .unwrap_or(self.content.len());
        (start, end)
    }

    /// A no-op before the first paint (relies on the layout cached at the last paint).
    fn move_vertical(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        let computed = self.last_layout.as_ref().and_then(|layout| {
            let caret = layout.point_for_offset(self.cursor_offset())?;
            let goal_x = self.preferred_x.unwrap_or(caret.x);
            let lh = layout.line_height;
            let target_y = if down {
                caret.y + lh * 1.5
            } else {
                caret.y - lh * 0.5
            };
            let mut clamped_y = target_y;
            if clamped_y < px(0.0) {
                clamped_y = px(0.0);
            }
            let max_y = layout.total_height - px(1.0);
            if max_y > px(0.0) && clamped_y > max_y {
                clamped_y = max_y;
            }
            let new = layout.offset_for_point(point(goal_x, clamped_y));
            Some((goal_x, new))
        });
        if let Some((goal_x, new)) = computed {
            self.preferred_x = Some(goal_x);
            if extend {
                self.select_to(new, cx);
            } else {
                self.move_to(new, cx);
            }
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.follow_caret = true;
        self.blink_visible = true;
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(layout)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        let x = position.x - bounds.left() - self.gutter_width();
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

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.follow_caret = true;
        self.blink_visible = true;
        cx.notify();
    }
}

impl EntityInputHandler for TextArea {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = range_from_utf16(&self.content, &range_utf16);
        actual_range.replace(range_to_utf16(&self.content, &range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: range_to_utf16(&self.content, &self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| range_to_utf16(&self.content, range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| range_from_utf16(&self.content, range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        self.follow_caret = true;
        self.blink_visible = true;
        cx.emit(InputEvent::Changed(self.content.clone()));
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| range_from_utf16(&self.content, range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if new_text.is_empty() {
            self.marked_range = None;
        } else {
            self.marked_range = Some(range.start..range.start + new_text.len());
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| range_from_utf16(&self.content, range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        self.follow_caret = true;
        self.blink_visible = true;
        cx.emit(InputEvent::Changed(self.content.clone()));
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = range_from_utf16(&self.content, &range_utf16);
        let start = layout.point_for_offset(range.start)?;
        let lh = layout.line_height;
        let gutter_w = self.gutter_width();
        Some(Bounds::from_corners(
            point(
                bounds.left() + gutter_w + start.x,
                bounds.top() + start.y - self.scroll_offset,
            ),
            point(
                bounds.left() + gutter_w + start.x,
                bounds.top() + start.y - self.scroll_offset + lh,
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.index_for_mouse_position(point);
        Some(offset_to_utf16(&self.content, offset))
    }
}

/// Splits every run overlapping `[start, end)` at those boundaries, then runs `f`
/// over the pieces that fall inside the range, so a selection/marked attribute can
/// be layered on top of already-colored foreground runs.
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
    syntax: Option<&[(usize, Hsla)]>,
) -> Vec<TextRun> {
    let make = |len: usize, color: Hsla| TextRun {
        len,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };

    let mut runs: Vec<TextRun> = match syntax {
        Some(spans) if !spans.is_empty() => spans
            .iter()
            .map(|(len, color)| make(*len, *color))
            .collect(),
        _ => vec![make(text.len(), base_color)],
    };

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

fn json_literal_at(chars: &[(usize, char)], i: usize) -> Option<usize> {
    for word in ["true", "false", "null"] {
        let wl = word.len();
        if i + wl <= chars.len() && word.chars().enumerate().all(|(k, wc)| chars[i + k].1 == wc) {
            return Some(wl);
        }
    }
    None
}

/// Byte-length foreground runs covering the whole buffer: object keys, string
/// values, numbers, and `true`/`false`/`null` literals each get their design hue;
/// everything else (punctuation, whitespace) stays the primary text color.
pub fn json_syntax_runs(text: &str, palette: &ForgePalette) -> Vec<(usize, Hsla)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let total = text.len();
    let byte_at = |i: usize| if i < n { chars[i].0 } else { total };
    let mut runs: Vec<(usize, Hsla)> = Vec::new();
    let mut i = 0;
    while i < n {
        let c = chars[i].1;
        let start = i;
        let hue: Hsla = if c.is_whitespace() {
            while i < n && chars[i].1.is_whitespace() {
                i += 1;
            }
            palette.text_secondary.into()
        } else if c == '"' {
            i += 1;
            while i < n {
                match chars[i].1 {
                    '\\' => i += 2,
                    '"' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
            i = i.min(n);
            let mut j = i;
            while j < n && chars[j].1.is_whitespace() {
                j += 1;
            }
            if j < n && chars[j].1 == ':' {
                palette.info.into()
            } else {
                palette.success.into()
            }
        } else if c == '-' || c.is_ascii_digit() {
            i += 1;
            while i < n
                && (chars[i].1.is_ascii_digit()
                    || matches!(chars[i].1, '.' | 'e' | 'E' | '+' | '-'))
            {
                i += 1;
            }
            palette.bits.into()
        } else if let Some(word_len) = json_literal_at(&chars, i) {
            i += word_len;
            palette.brand.into()
        } else {
            i += 1;
            palette.text_primary.into()
        };
        let len = byte_at(i) - byte_at(start);
        if len > 0 {
            runs.push((len, hue));
        }
    }
    runs
}

pub fn json_highlighted(text: impl Into<SharedString>, palette: &ForgePalette) -> StyledText {
    let text = text.into();
    let mut offset = 0usize;
    let highlights: Vec<(Range<usize>, HighlightStyle)> = json_syntax_runs(&text, palette)
        .into_iter()
        .map(|(len, color)| {
            let start = offset;
            offset += len;
            (start..offset, HighlightStyle::from(color))
        })
        .collect();
    StyledText::new(text).with_highlights(highlights)
}

fn is_rhai_keyword(word: &str) -> bool {
    matches!(
        word,
        "fn" | "let"
            | "const"
            | "if"
            | "else"
            | "return"
            | "for"
            | "in"
            | "while"
            | "loop"
            | "break"
            | "continue"
            | "true"
            | "false"
            | "switch"
            | "import"
            | "as"
            | "throw"
            | "private"
    )
}

/// Byte-length foreground runs for a Rhai buffer: keywords, strings, numbers,
/// line comments, and call-position identifiers each get their design hue; other
/// identifiers stay primary and punctuation/whitespace stay muted.
fn rhai_syntax_runs(text: &str, palette: &ForgePalette) -> Vec<(usize, Hsla)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let total = text.len();
    let byte_at = |i: usize| if i < n { chars[i].0 } else { total };
    let mut runs: Vec<(usize, Hsla)> = Vec::new();
    let mut i = 0;
    while i < n {
        let c = chars[i].1;
        let start = i;
        let hue: Hsla = if c == '/' && i + 1 < n && chars[i + 1].1 == '/' {
            while i < n && chars[i].1 != '\n' {
                i += 1;
            }
            palette.code_comment.into()
        } else if c == '"' {
            i += 1;
            while i < n {
                match chars[i].1 {
                    '\\' => i += 2,
                    '"' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
            i = i.min(n);
            palette.code_str.into()
        } else if c.is_ascii_digit() {
            i += 1;
            while i < n
                && (chars[i].1.is_ascii_digit() || matches!(chars[i].1, '.' | '_' | 'e' | 'E'))
            {
                i += 1;
            }
            palette.code_num.into()
        } else if c.is_alphabetic() || c == '_' {
            i += 1;
            while i < n && (chars[i].1.is_alphanumeric() || chars[i].1 == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().map(|(_, ch)| *ch).collect();
            let mut j = i;
            while j < n && chars[j].1 == ' ' {
                j += 1;
            }
            if is_rhai_keyword(&word) {
                palette.code_keyword.into()
            } else if j < n && chars[j].1 == '(' {
                palette.code_fn.into()
            } else {
                palette.text_primary.into()
            }
        } else if c.is_whitespace() {
            while i < n && chars[i].1.is_whitespace() {
                i += 1;
            }
            palette.text_secondary.into()
        } else {
            i += 1;
            palette.text_secondary.into()
        };
        let len = byte_at(i) - byte_at(start);
        if len > 0 {
            runs.push((len, hue));
        }
    }
    runs
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
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let marked_range = input.marked_range.clone();
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
        let syntax_runs = if content.is_empty() {
            None
        } else {
            match input.syntax {
                SyntaxMode::Json => Some(json_syntax_runs(&content, &palette)),
                SyntaxMode::Rhai => Some(rhai_syntax_runs(&content, &palette)),
                SyntaxMode::None => None,
            }
        };
        let runs = build_runs(
            &display_text,
            base_color,
            style.font(),
            &selection,
            with_alpha(palette.brand, 0.25).into(),
            marked_range.as_ref(),
            syntax_runs.as_deref(),
        );

        let gutter_w = if input.gutter { GUTTER_W } else { px(0.0) };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let text_width = bounds.size.width - gutter_w;
        let wrap_width = if text_width > px(0.0) {
            Some(text_width)
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

        // Keep the caret within the viewport.
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
                        bounds.left() + gutter_w + caret.x,
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

        let (gutter, gutter_marks, faint, warning) = {
            let input = self.input.read(cx);
            (
                input.gutter,
                input.gutter_marks.clone(),
                input.palette.text_faint,
                input.palette.warning,
            )
        };
        let gutter_w = if gutter { GUTTER_W } else { px(0.0) };

        // Paint selection backgrounds first, then the glyphs, per paragraph.
        for (i, line) in layout.lines.iter().enumerate() {
            let origin = point(
                bounds.left() + gutter_w,
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

        if gutter {
            let style = window.text_style();
            let font = style.font();
            let font_size = style.font_size.to_pixels(window.rem_size());
            for i in 0..layout.para_tops.len() {
                let y = bounds.top() + layout.para_tops[i] - scroll_offset;
                let marked = gutter_marks.contains(&i);
                let (label, color): (SharedString, Hsla) = if marked {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(bounds.left(), y),
                            size(GUTTER_ACCENT_W, layout.line_height),
                        ),
                        warning,
                    ));
                    (GUTTER_MARK.into(), warning.into())
                } else {
                    ((i + 1).to_string().into(), faint.into())
                };
                let run = TextRun {
                    len: label.len(),
                    font: font.clone(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(
                    label,
                    font_size,
                    std::slice::from_ref(&run),
                    None,
                );
                let x = bounds.left() + GUTTER_W - GUTTER_PAD_R - shaped.width();
                let _ = shaped.paint(
                    point(x, y),
                    layout.line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        }

        if focus_handle.is_focused(window)
            && self.input.read(cx).blink_visible
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
        let focused = self.focus_handle.is_focused(window);
        self.focused_cached = focused;
        let border_color = if self.read_only {
            self.palette.disabled
        } else if focused {
            self.palette.border_active
        } else {
            self.palette.border_input
        };
        let text_color = if self.read_only {
            self.palette.text_muted
        } else {
            self.palette.text_primary
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
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .w_full()
            .overflow_hidden()
            .font_family(self.font_family)
            .text_size(self.font_size)
            .text_color(text_color)
            .line_height(self.font_size * 1.5);
        let field = if self.fill {
            field
                .flex_1()
                .min_h_0()
                .py(spacing(Spacing::Xs, self.density))
        } else {
            field
                .h(self.height)
                .px(spacing(Spacing::Sm, self.density))
                .py(spacing(Spacing::Xs, self.density))
                .bg(surface)
                .border(BORDER_THIN)
                .border_color(border_color)
                .rounded(radius(Radius::Md))
        };
        field.child(AreaElement { input: cx.entity() })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // Seed a headless TextArea with `content`, then run `f` against its private editing
    // methods and return what `f` observes. gpui's TestAppContext backs the window with a
    // headless TestWindow (NoopTextSystem, no GPU, no paint scheduling, no network) - the
    // sanctioned in-process harness. Tests that exercise caret geometry (Up/Down) must
    // instead force a paint first (see `with_painted_area`), because `move_vertical` reads
    // the layout cached at the last paint and is a no-op before it.
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

    // Like `with_area`, but paints the window once after seeding so `last_layout` is
    // populated with the NoopTextSystem's deterministic monospaced shaping (every ASCII
    // glyph advances one fixed em-width). Vertical caret motion needs that cached layout.
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
        cx.run_until_parked(); // first paint caches the shaped layout used by Up/Down
        window
            .update(cx, |area, window, cx| f(area, window, cx))
            .unwrap()
    }

    #[gpui::test]
    fn enter_inserts_a_newline_at_the_cursor_instead_of_submitting(cx: &mut gpui::TestAppContext) {
        // The defining multi-line behavior: Enter splits the buffer with a '\n' at the
        // caret and advances past it, where the single-line input would emit a submit and
        // leave the text untouched.
        let (content, cursor) = with_area(cx, "abcd", |area, window, cx| {
            area.move_to(2, cx);
            area.insert_newline(&InsertNewline, window, cx);
            (area.content().to_string(), area.cursor_offset())
        });
        assert_eq!(content, "ab\ncd");
        assert_eq!(cursor, 3); // caret sits just after the inserted '\n'
    }

    #[gpui::test]
    fn down_holds_the_goal_column_across_a_shorter_intervening_line(cx: &mut gpui::TestAppContext) {
        // The bug-prone case. Caret starts at column 6 of a long line; the middle line is
        // one char, so Down must clamp there, THEN the next Down must return to column 6 -
        // proving the goal column survived the clamp. A naive impl that overwrites the goal
        // column with the clamped x would land at column 1 on the third line (offset 13),
        // not column 6 (offset 18).
        let stops = with_painted_area(cx, "long line\nx\nanother line", |area, window, cx| {
            area.move_to(6, cx); // column 6 of "long line"
            let mut seen = Vec::new();
            area.down(&Down, window, cx);
            seen.push(area.cursor_offset()); // clamps onto the 1-char middle line
            area.down(&Down, window, cx);
            seen.push(area.cursor_offset()); // goal column 6 restored on the third line
            seen
        });
        assert_eq!(stops, vec![11usize, 18]);
    }

    #[gpui::test]
    fn up_holds_the_goal_column_across_a_shorter_intervening_line(cx: &mut gpui::TestAppContext) {
        // Symmetric to the Down case, exercising the `caret.y - lh*0.5` branch.
        let stops = with_painted_area(cx, "long line\nx\nanother line", |area, window, cx| {
            area.move_to(18, cx); // column 6 of "another line"
            let mut seen = Vec::new();
            area.up(&Up, window, cx);
            seen.push(area.cursor_offset()); // clamps onto the 1-char middle line
            area.up(&Up, window, cx);
            seen.push(area.cursor_offset()); // goal column 6 restored on the first line
            seen
        });
        assert_eq!(stops, vec![11usize, 6]);
    }

    #[gpui::test]
    fn paste_preserves_embedded_newlines(cx: &mut gpui::TestAppContext) {
        // A text area pastes multi-line clipboard text verbatim; the single-line input
        // would flatten the '\n's away.
        let (content, cursor) = with_area(cx, "", |area, window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("a\nb\nc".to_string()));
            area.paste(&Paste, window, cx);
            (area.content().to_string(), area.cursor_offset())
        });
        assert_eq!(content, "a\nb\nc");
        assert_eq!(cursor, 5); // caret advances past the whole pasted block
    }

    #[gpui::test]
    fn home_moves_to_the_start_of_the_current_hard_line(cx: &mut gpui::TestAppContext) {
        // Home binds to the hard line the caret is on, not the whole buffer: from inside
        // "second" it lands at that line's first byte (6), never at 0.
        let cursor = with_area(cx, "first\nsecond\nthird", |area, window, cx| {
            area.move_to(9, cx); // mid-"second"
            area.home(&Home, window, cx);
            area.cursor_offset()
        });
        assert_eq!(cursor, 6);
    }

    #[gpui::test]
    fn end_moves_to_the_end_of_the_current_hard_line(cx: &mut gpui::TestAppContext) {
        // End binds to the hard line, landing before the trailing '\n' (12), never at the
        // buffer end (18).
        let cursor = with_area(cx, "first\nsecond\nthird", |area, window, cx| {
            area.move_to(9, cx); // mid-"second"
            area.end(&End, window, cx);
            area.cursor_offset()
        });
        assert_eq!(cursor, 12);
    }
}
