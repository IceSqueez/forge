use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, Hsla,
    KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    Pixels, Point, Rgba, ShapedLine, SharedString, Style, TextAlign, TextRun, UTF16Selection,
    UnderlineStyle, Window, actions, div, fill, point, prelude::*, px, relative, size,
};

use crate::icons::{Icon, icon};
use crate::palette::{CATPPUCCIN_MOCHA, ForgePalette, with_alpha};
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, Radius,
    Spacing, radius, spacing,
};

const KEY_CONTEXT: &str = "ForgeTextInput";

const CARET_BLINK_MS: u64 = 530;

fn spawn_caret_blink(cx: &mut Context<TextInput>) {
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

actions!(
    forge_text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Submit,
        Cancel,
        Paste,
        Cut,
        Copy,
    ]
);

/// The binary MUST call this once at boot, or navigation and editing keys are dead.
pub fn bind_text_input_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some(KEY_CONTEXT)),
        KeyBinding::new("delete", Delete, Some(KEY_CONTEXT)),
        KeyBinding::new("left", Left, Some(KEY_CONTEXT)),
        KeyBinding::new("right", Right, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(KEY_CONTEXT)),
        KeyBinding::new("home", Home, Some(KEY_CONTEXT)),
        KeyBinding::new("end", End, Some(KEY_CONTEXT)),
        KeyBinding::new("enter", Submit, Some(KEY_CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(KEY_CONTEXT)),
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

#[derive(Clone, Debug)]
pub enum InputEvent {
    Changed(SharedString),
    Submitted(SharedString),
    Cancelled,
}

pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    scroll_offset: Pixels,
    is_selecting: bool,
    palette: ForgePalette,
    density: Density,
    font_size: Pixels,
    read_only: bool,
    secure: bool,
    leading_icon: Option<(Icon, Rgba)>,
    prefix: Option<SharedString>,
    accent: Option<Rgba>,
    on_surface: bool,
    static_chrome: Option<(Rgba, Radius)>,
    plain: bool,
    mono: bool,
    invalid: bool,
    blink_visible: bool,
    focused_cached: bool,
}

impl EventEmitter<InputEvent> for TextInput {}

impl TextInput {
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
            palette: CATPPUCCIN_MOCHA,
            density: Density::Cozy,
            font_size: FONT_XS,
            read_only: false,
            secure: false,
            leading_icon: None,
            prefix: None,
            accent: None,
            on_surface: false,
            static_chrome: None,
            plain: false,
            mono: false,
            invalid: false,
            blink_visible: true,
            focused_cached: false,
        }
    }

    #[must_use]
    pub fn plain(mut self) -> Self {
        self.plain = true;
        self
    }

    #[must_use]
    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
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

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Masks display only - `content()` and events keep the real value; clipboard copy/cut is suppressed while masked.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    pub fn leading_icon(mut self, glyph: Icon, tint: Rgba) -> Self {
        self.leading_icon = Some((glyph, tint));
        self
    }

    pub fn prefix(mut self, prefix: impl Into<SharedString>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Tints the typed text and the focused border (for variable / accent-typed fields).
    pub fn accent(mut self, accent: Rgba) -> Self {
        self.accent = Some(accent);
        self
    }

    pub fn on_surface(mut self) -> Self {
        self.on_surface = true;
        self
    }

    pub fn static_chrome(mut self, border: Rgba, corner: Radius) -> Self {
        self.static_chrome = Some((border, corner));
        self
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn set_palette(&mut self, palette: ForgePalette, cx: &mut Context<Self>) {
        self.palette = palette;
        cx.notify();
    }

    pub fn set_static_chrome(&mut self, chrome: Option<(Rgba, Radius)>) {
        self.static_chrome = chrome;
    }

    /// Flips display masking at runtime (for a reveal toggle). Masking hides display
    /// only - `content()` keeps the real value either way.
    pub fn set_secure(&mut self, secure: bool, cx: &mut Context<Self>) {
        if self.secure != secure {
            self.secure = secure;
            cx.notify();
        }
    }

    /// Forces a red error border that wins over the idle and focus border colors.
    pub fn set_invalid(&mut self, invalid: bool, cx: &mut Context<Self>) {
        if self.invalid != invalid {
            self.invalid = invalid;
            cx.notify();
        }
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    pub fn set_content(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = text.into();
        let end = self.content.len();
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set_content("", cx);
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(InputEvent::Submitted(self.content.clone()));
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(InputEvent::Cancelled);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.secure {
            return;
        }
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
        if !self.secure {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
        }
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
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

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
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
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        let index = line.closest_index_for_x(position.x - bounds.left() + self.scroll_offset);
        if self.secure {
            crate::text_edit::content_offset_for_mask(&self.content, index)
        } else {
            index
        }
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
        self.blink_visible = true;
        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        crate::text_edit::offset_from_utf16(&self.content, offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        crate::text_edit::offset_to_utf16(&self.content, offset)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        crate::text_edit::previous_grapheme_boundary(&self.content, offset)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        crate::text_edit::next_grapheme_boundary(&self.content, offset)
    }
}

pub fn search_input(
    placeholder: impl Into<SharedString>,
    palette: ForgePalette,
    cx: &mut Context<TextInput>,
) -> TextInput {
    TextInput::new(placeholder, cx)
        .with_palette(palette)
        .leading_icon(Icon::Search, palette.text_muted)
        .static_chrome(palette.border_regular, Radius::Sm)
}

pub fn search_input_on_surface(
    placeholder: impl Into<SharedString>,
    palette: ForgePalette,
    cx: &mut Context<TextInput>,
) -> TextInput {
    search_input(placeholder, palette, cx).on_surface()
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
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
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
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
            .map(|range_utf16| self.range_from_utf16(range_utf16))
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
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

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
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let (start, end) = if self.secure {
            (
                crate::text_edit::mask_offset(&self.content, range.start),
                crate::text_edit::mask_offset(&self.content, range.end),
            )
        } else {
            (range.start, range.end)
        };
        Some(Bounds::from_corners(
            point(
                bounds.left() - self.scroll_offset + last_layout.x_for_index(start),
                bounds.top(),
            ),
            point(
                bounds.left() - self.scroll_offset + last_layout.x_for_index(end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let index = last_layout.index_for_x(point.x - line_point.x + self.scroll_offset)?;
        let utf8_index = if self.secure {
            crate::text_edit::content_offset_for_mask(&self.content, index)
        } else {
            index
        };
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    scroll_offset: Pixels,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
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
        style.size.height = window.line_height().into();
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
        let cursor = input.cursor_offset();
        let palette = input.palette;
        let secure = input.secure;
        let mut scroll_offset = input.scroll_offset;
        let style = window.text_style();

        let (display_text, text_color): (SharedString, Hsla) = if content.is_empty() {
            (input.placeholder.clone(), palette.text_muted.into())
        } else if secure {
            (
                SharedString::from(crate::text_edit::mask_graphemes(&content)),
                style.color,
            )
        } else {
            (content.clone(), style.color)
        };

        // Caret and selection offsets index the real buffer; while masked the shaped line
        // is bullets, so each real byte offset maps to its bullet-string counterpart.
        let display_index = |offset: usize| -> usize {
            if secure {
                crate::text_edit::mask_offset(&content, offset)
            } else {
                offset
            }
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref().filter(|_| !secure) {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text.clone(), font_size, &runs, None);

        let cursor_x = line.x_for_index(display_index(cursor));
        let full_width = line.x_for_index(display_text.len());
        let width = bounds.size.width;

        if cursor_x - scroll_offset > width {
            scroll_offset = cursor_x - width;
        }
        if cursor_x - scroll_offset < px(0.0) {
            scroll_offset = cursor_x;
        }
        if scroll_offset < px(0.0) {
            scroll_offset = px(0.0);
        }
        let max_scroll = full_width - width;
        if max_scroll > px(0.0) && scroll_offset > max_scroll {
            scroll_offset = max_scroll;
        } else if max_scroll <= px(0.0) {
            scroll_offset = px(0.0);
        }

        let origin_x = bounds.left() - scroll_offset;
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(origin_x + cursor_x, bounds.top()),
                        size(px(1.5), bounds.bottom() - bounds.top()),
                    ),
                    palette.text_primary,
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            origin_x + line.x_for_index(display_index(selected_range.start)),
                            bounds.top(),
                        ),
                        point(
                            origin_x + line.x_for_index(display_index(selected_range.end)),
                            bounds.bottom(),
                        ),
                    ),
                    with_alpha(palette.brand, 0.25),
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
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
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let scroll_offset = prepaint.scroll_offset;
        let Some(line) = prepaint.line.take() else {
            return;
        };
        let origin = point(bounds.left() - scroll_offset, bounds.top());
        let _ = line.paint(
            origin,
            window.line_height(),
            TextAlign::Left,
            None,
            window,
            cx,
        );

        if focus_handle.is_focused(window)
            && self.input.read(cx).blink_visible
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
            input.scroll_offset = scroll_offset;
        });
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        self.focused_cached = focused;
        let (border_color, corner) = match self.static_chrome {
            Some((border, corner)) => (border, corner),
            None => {
                let border = if self.read_only {
                    self.palette.disabled
                } else if focused {
                    self.accent.unwrap_or(self.palette.border_active)
                } else {
                    self.palette.border_input
                };
                (border, Radius::Md)
            }
        };
        let border_color = if self.invalid {
            self.palette.random
        } else {
            border_color
        };
        let text_color = if self.read_only {
            self.palette.text_muted
        } else if let Some(accent) = self.accent {
            accent
        } else {
            self.palette.text_primary
        };
        let surface = if self.on_surface {
            self.palette.elevated
        } else {
            self.palette.shell
        };

        let mut field = div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .flex()
            .items_center()
            .overflow_hidden();
        if !self.plain {
            field = field
                .px(spacing(Spacing::Sm, self.density))
                .py(spacing(Spacing::Xs, self.density))
                .bg(surface)
                .border(BORDER_THIN)
                .border_color(border_color)
                .rounded(radius(corner));
        }
        let font_family = if self.mono {
            DEFAULT_MONO_FAMILY
        } else {
            DEFAULT_BODY_FAMILY
        };
        let mut field = field
            .font_family(font_family)
            .text_size(self.font_size)
            .text_color(text_color)
            .line_height(self.font_size * 1.5);

        let content = div()
            .flex_1()
            .overflow_hidden()
            .child(TextElement { input: cx.entity() });
        if self.leading_icon.is_none() && self.prefix.is_none() {
            return field.child(content);
        }
        field = field.gap(spacing(Spacing::Xs, self.density));
        if let Some((glyph, tint)) = self.leading_icon {
            field = field.child(icon(glyph, FONT_SM, tint));
        }
        if let Some(prefix) = self.prefix.clone() {
            field = field.child(
                div()
                    .flex_none()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(self.font_size)
                    .text_color(self.palette.text_faint)
                    .child(prefix),
            );
        }
        field.child(content)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn with_input<R>(
        cx: &mut gpui::TestAppContext,
        content: &str,
        f: impl FnOnce(&mut TextInput, &mut Window, &mut Context<TextInput>) -> R,
    ) -> R {
        let window = cx.add_window(|_window, cx| TextInput::new("placeholder", cx));
        window
            .update(cx, |input, window, cx| {
                input.set_content(content.to_string(), cx);
                f(input, window, cx)
            })
            .unwrap()
    }

    #[gpui::test]
    fn backspace_deletes_the_whole_previous_grapheme_cluster(cx: &mut gpui::TestAppContext) {
        // Why: previous_boundary must step one extended grapheme cluster, never a
        // byte or a codepoint. A byte-based cursor slices mid-emoji (panic on the
        // non-char-boundary slice); a codepoint-based one tears a ZWJ / flag
        // cluster apart. set_content parks the caret at the end, so one backspace
        // removes the final cluster.
        for (before, after) in [
            ("a😀", "a"),    // 4-byte astral emoji removed whole
            ("café", "caf"), // 2-byte é removed whole
            ("🇺🇦", ""),      // regional-indicator flag: one cluster, 8 bytes
            ("👨‍👩‍👧", ""),      // ZWJ family: one cluster, many codepoints
        ] {
            let content = with_input(cx, before, |input, window, cx| {
                input.backspace(&Backspace, window, cx);
                input.content().to_string()
            });
            assert_eq!(content, after, "backspace on {before:?}");
        }
    }

    #[gpui::test]
    fn delete_removes_the_whole_next_grapheme_cluster(cx: &mut gpui::TestAppContext) {
        for (before, after) in [("😀a", "a"), ("🇺🇦b", "b"), ("👨‍👩‍👧z", "z")] {
            let content = with_input(cx, before, |input, window, cx| {
                input.home(&Home, window, cx);
                input.delete(&Delete, window, cx);
                input.content().to_string()
            });
            assert_eq!(content, after, "delete-forward on {before:?}");
        }
    }

    #[gpui::test]
    fn cursor_left_lands_only_on_grapheme_boundaries(cx: &mut gpui::TestAppContext) {
        // "a😀b": grapheme starts sit at byte offsets 0, 1, 5 (total len 6). The
        // caret must never rest at 2, 3 or 4 (inside the emoji's 4 bytes).
        let stops = with_input(cx, "a😀b", |input, window, cx| {
            input.end(&End, window, cx);
            let mut seen = Vec::new();
            for _ in 0..3 {
                input.left(&Left, window, cx);
                seen.push(input.cursor_offset());
            }
            seen
        });
        assert_eq!(stops, vec![5usize, 1, 0]);
    }

    #[gpui::test]
    fn cursor_right_lands_only_on_grapheme_boundaries(cx: &mut gpui::TestAppContext) {
        let stops = with_input(cx, "a😀b", |input, window, cx| {
            input.home(&Home, window, cx);
            let mut seen = Vec::new();
            for _ in 0..3 {
                input.right(&Right, window, cx);
                seen.push(input.cursor_offset());
            }
            seen
        });
        assert_eq!(stops, vec![1usize, 5, 6]);
    }

    #[gpui::test]
    fn typing_inserts_text_at_the_cursor_and_advances_it(cx: &mut gpui::TestAppContext) {
        let (content, cursor) = with_input(cx, "helo", |input, window, cx| {
            input.home(&Home, window, cx);
            input.right(&Right, window, cx);
            input.right(&Right, window, cx);
            input.replace_text_in_range(None, "l", window, cx);
            (input.content().to_string(), input.cursor_offset())
        });
        assert_eq!(content, "hello");
        assert_eq!(cursor, 3);
    }

    #[gpui::test]
    fn select_all_then_type_replaces_the_entire_content(cx: &mut gpui::TestAppContext) {
        let content = with_input(cx, "hello world", |input, window, cx| {
            input.select_all(&SelectAll, window, cx);
            input.replace_text_in_range(None, "X", window, cx);
            input.content().to_string()
        });
        assert_eq!(content, "X");
    }

    #[gpui::test]
    fn backspace_with_a_selection_deletes_exactly_the_selection(cx: &mut gpui::TestAppContext) {
        // A non-empty selection means backspace deletes the selection verbatim and
        // does NOT swallow an extra grapheme before it.
        let content = with_input(cx, "hello", |input, window, cx| {
            input.home(&Home, window, cx);
            for _ in 0..3 {
                input.select_right(&SelectRight, window, cx);
            }
            input.backspace(&Backspace, window, cx);
            input.content().to_string()
        });
        assert_eq!(content, "lo");
    }

    #[gpui::test]
    fn backspace_at_the_start_is_a_no_op(cx: &mut gpui::TestAppContext) {
        let content = with_input(cx, "hi", |input, window, cx| {
            input.home(&Home, window, cx);
            input.backspace(&Backspace, window, cx);
            input.content().to_string()
        });
        assert_eq!(content, "hi");
    }

    #[gpui::test]
    fn delete_at_the_end_is_a_no_op(cx: &mut gpui::TestAppContext) {
        let content = with_input(cx, "hi", |input, window, cx| {
            input.end(&End, window, cx);
            input.delete(&Delete, window, cx);
            input.content().to_string()
        });
        assert_eq!(content, "hi");
    }

    #[gpui::test]
    fn read_only_input_rejects_every_edit_path(cx: &mut gpui::TestAppContext) {
        // Why: read_only is a hard invariant - no edit action nor the IME replace
        // path may mutate the buffer.
        let window = cx.add_window(|_window, cx| TextInput::new("", cx).read_only(true));
        let content = window
            .update(cx, |input, window, cx| {
                input.set_content("locked".to_string(), cx);
                input.backspace(&Backspace, window, cx);
                input.home(&Home, window, cx);
                input.delete(&Delete, window, cx);
                input.replace_text_in_range(None, "x", window, cx);
                input.content().to_string()
            })
            .unwrap();
        assert_eq!(content, "locked");
    }

    #[gpui::test]
    fn utf16_offsets_map_across_the_astral_plane(cx: &mut gpui::TestAppContext) {
        // "a😀b": 😀 is 4 UTF-8 bytes but 2 UTF-16 code units (a surrogate pair).
        // The IME coordinate mapping must count surrogate pairs, not chars - a
        // char-count impl would report byte 5 as utf16 2 instead of 3.
        with_input(cx, "a😀b", |input, _window, _cx| {
            for (byte, utf16) in [(0usize, 0usize), (1, 1), (5, 3), (6, 4)] {
                assert_eq!(input.offset_to_utf16(byte), utf16, "byte {byte} -> utf16");
                assert_eq!(
                    input.offset_from_utf16(utf16),
                    byte,
                    "utf16 {utf16} -> byte"
                );
            }
        });
    }
}
