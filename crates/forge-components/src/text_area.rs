//! Multi-line text editor entity — the wrapping, vertically-scrolling counterpart
//! to [`crate::text_input::TextInput`]. Same styled surface (shell fill, focus-reactive
//! border, `Radius::Md`), but Enter inserts a newline, Up/Down move the caret across
//! visual rows, long paragraphs soft-wrap to the field width, and content taller than
//! the fixed viewport scrolls vertically.
//!
//! Like [`crate::text_input::TextInput`] this is a stateful `Entity` view (holds focus +
//! buffer + selection), NOT a stateless `RenderOnce`. The screen creates and holds
//! `Entity<TextArea>` and reacts to edits via `cx.subscribe(&area, …)` on the shared
//! [`InputEvent`]. Grapheme/UTF-16 helpers are shared with the single-line input through
//! [`crate::text_edit`].

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, Hsla,
    KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    Pixels, Point, SharedString, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window,
    WrappedLine, actions, div, fill, point, prelude::*, px, relative, size,
};

use crate::palette::{CATPPUCCIN_MOCHA, ForgePalette, with_alpha};
use crate::text_edit::{
    next_grapheme_boundary, offset_to_utf16, previous_grapheme_boundary, range_from_utf16,
    range_to_utf16,
};
use crate::text_input::InputEvent;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, Density, FONT_XS, Radius, Spacing, radius, spacing,
};

const KEY_CONTEXT: &str = "ForgeTextArea";

/// Default visible height of the editable viewport (content scrolls within it). Mirrors
/// the fixed height the retiring iced multi-line field used for long-form / JSON values.
const DEFAULT_AREA_HEIGHT: Pixels = px(130.0);

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

/// Installs the editing key bindings for every [`TextArea`], scoped to the area's key
/// context so they never fire outside a focused field. The binary MUST call this once at
/// boot — without it only literal character typing works; navigation, newline insertion
/// and editing keys are dead. Distinct from `bind_text_input_keys`: Enter inserts a
/// newline here (single-line inputs submit), and Up/Down move across rows.
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

/// A shaped snapshot of the buffer as laid out at the last paint: one [`WrappedLine`]
/// per hard-newline-delimited paragraph (each of which may itself soft-wrap to several
/// visual rows). Cached so between-frame hit-testing (mouse, Up/Down) can map global byte
/// offsets ↔ pixel positions without re-shaping.
struct AreaLayout {
    /// One entry per `\n`-delimited paragraph, in document order.
    lines: Vec<WrappedLine>,
    /// `para_tops[i]` = the y of paragraph `i`'s top edge, in content space (0 = start).
    para_tops: Vec<Pixels>,
    /// `para_byte_starts[i]` = the global byte offset where paragraph `i` begins.
    para_byte_starts: Vec<usize>,
    line_height: Pixels,
    total_height: Pixels,
}

impl AreaLayout {
    /// The pixel position (content space, pre-scroll) of the caret for global byte
    /// `offset`, or `None` if the layout is empty. The caller adds the viewport origin
    /// and subtracts the scroll offset.
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

    /// The global byte offset closest to `p` (content space, pre-scroll). Clamps into the
    /// nearest paragraph and grapheme boundary.
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

/// Multi-line editor entity. Create + hold `Entity<TextArea>` on the screen; subscribe to
/// [`InputEvent`] for edits (only `Changed` is emitted — a multi-line field has no submit).
pub struct TextArea {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<AreaLayout>,
    last_bounds: Option<Bounds<Pixels>>,
    /// Vertical scroll offset (content scrolls up as this grows).
    scroll_offset: Pixels,
    is_selecting: bool,
    /// Goal column for Up/Down: the x the caret aims for across a run of vertical moves,
    /// so it does not drift on ragged rows. Cleared by any horizontal move or edit.
    preferred_x: Option<Pixels>,
    palette: ForgePalette,
    density: Density,
    font_size: Pixels,
    read_only: bool,
    height: Pixels,
    on_surface: bool,
}

impl EventEmitter<InputEvent> for TextArea {}

impl TextArea {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
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
            palette: CATPPUCCIN_MOCHA,
            density: Density::Cozy,
            font_size: FONT_XS,
            read_only: false,
            height: DEFAULT_AREA_HEIGHT,
            on_surface: false,
        }
    }

    pub fn with_palette(mut self, palette: ForgePalette) -> Self {
        self.palette = palette;
        self
    }

    pub fn with_density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    pub fn with_font_size(mut self, size: Pixels) -> Self {
        self.font_size = size;
        self
    }

    /// Overrides the fixed viewport height (default [`DEFAULT_AREA_HEIGHT`]).
    pub fn with_height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Fills the field with the palette's `elevated` surface instead of the base `shell`,
    /// for a field on a raised panel.
    pub fn on_surface(mut self) -> Self {
        self.on_surface = true;
        self
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn set_palette(&mut self, palette: ForgePalette, cx: &mut Context<Self>) {
        self.palette = palette;
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

    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus_handle);
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
        // Unlike the single-line input, newlines are preserved — pasting multi-line text
        // is the whole point of a text area.
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

    /// The start (byte after the preceding `\n`, or 0) and end (byte before the next
    /// `\n`, or `content.len()`) of the hard line the caret is on — drives Home/End.
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

    /// Moves (or extends the selection) one visual row up or down, holding the goal
    /// column. Relies on the layout cached at the last paint; a no-op before first paint.
    fn move_vertical(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        let computed = self.last_layout.as_ref().and_then(|layout| {
            let caret = layout.point_for_offset(self.cursor_offset())?;
            let goal_x = self.preferred_x.unwrap_or(caret.x);
            let lh = layout.line_height;
            // Aim at the vertical centre of the neighbouring row.
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
        Some(Bounds::from_corners(
            point(
                bounds.left() + start.x,
                bounds.top() + start.y - self.scroll_offset,
            ),
            point(
                bounds.left() + start.x,
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

/// Builds the decorated runs for the shaped buffer: a marked (IME-composing) range is
/// underlined; otherwise a non-empty selection is filled with the translucent brand
/// colour so the wrapped-line painter draws it across every visual row automatically.
fn build_runs(
    text: &SharedString,
    base_color: Hsla,
    font: gpui::Font,
    selection: &Range<usize>,
    selection_bg: Hsla,
    marked_range: Option<&Range<usize>>,
) -> Vec<TextRun> {
    let base = TextRun {
        len: text.len(),
        font,
        color: base_color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };

    if let Some(marked) = marked_range {
        return vec![
            TextRun {
                len: marked.start,
                ..base.clone()
            },
            TextRun {
                len: marked.end - marked.start,
                underline: Some(UnderlineStyle {
                    color: Some(base.color),
                    thickness: px(1.0),
                    wavy: false,
                }),
                ..base.clone()
            },
            TextRun {
                len: text.len() - marked.end,
                ..base
            },
        ]
        .into_iter()
        .filter(|run| run.len > 0)
        .collect();
    }

    if !selection.is_empty() && selection.end <= text.len() {
        return vec![
            TextRun {
                len: selection.start,
                ..base.clone()
            },
            TextRun {
                len: selection.end - selection.start,
                background_color: Some(selection_bg),
                ..base.clone()
            },
            TextRun {
                len: text.len() - selection.end,
                ..base
            },
        ]
        .into_iter()
        .filter(|run| run.len > 0)
        .collect();
    }

    vec![base]
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
        // Fill the padded viewport the render `div` sizes; vertical overflow scrolls.
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

        // Selection colouring only applies over real content, never the placeholder.
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

        // Keep the caret within the viewport.
        let mut scroll_offset = input.scroll_offset;
        let view_h = bounds.size.height;
        let caret_point = if selected_range.is_empty() {
            layout.point_for_offset(cursor)
        } else {
            None
        };
        if let Some(caret) = caret_point {
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

        // Paint selection backgrounds first, then the glyphs, per paragraph.
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
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .h(self.height)
            .overflow_hidden()
            .px(spacing(Spacing::Sm, self.density))
            .py(spacing(Spacing::Xs, self.density))
            .bg(surface)
            .border(BORDER_THIN)
            .border_color(border_color)
            .rounded(radius(Radius::Md))
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(self.font_size)
            .text_color(text_color)
            .line_height(self.font_size * 1.5)
            .child(AreaElement { input: cx.entity() })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // Seed a headless TextArea with `content`, then run `f` against its private editing
    // methods and return what `f` observes. gpui's TestAppContext backs the window with a
    // headless TestWindow (NoopTextSystem, no GPU, no paint scheduling, no network) — the
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
        // one char, so Down must clamp there, THEN the next Down must return to column 6 —
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
