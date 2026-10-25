use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EventEmitter, FocusHandle, Focusable, GlobalElementId, Hsla, KeyBinding, LayoutId,
    PaintQuad, Pixels, Point, Rgba, ShapedLine, SharedString, Style, Subscription, TextAlign,
    TextRun, UnderlineStyle, Window, actions, div, fill, point, prelude::*, px, relative, size,
};

use crate::caret_blink::{CaretBlink, CaretHost, set_caret_blinking};
use crate::icons::{Icon, icon};
use crate::palette::{FORGE_DEFAULT, ForgePalette, with_alpha};
use crate::text_buffer::TextBuffer;
use crate::text_edit::{content_offset_for_mask, mask_graphemes, mask_offset};
use crate::text_editing::*;
use crate::tokens::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, Radius, Spacing, body_family, mono_family, radius,
    spacing,
};

const KEY_CONTEXT: &str = "ForgeTextInput";

actions!(
    forge_text_input,
    [Submit, Cancel, FocusNextField, FocusPreviousField]
);

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
        KeyBinding::new("tab", FocusNextField, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-tab", FocusPreviousField, Some(KEY_CONTEXT)),
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
    Blurred(SharedString),
    Cancelled,
}

pub struct TextInput {
    focus_handle: FocusHandle,
    buffer: TextBuffer,
    placeholder: SharedString,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    scroll_offset: Pixels,
    motion: CaretMotion,
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
    compact: bool,
    fixed_box: Option<(Pixels, Pixels)>,
    invalid: bool,
    caret: CaretBlink,
    committed: SharedString,
    blur_sub: Option<Subscription>,
}

impl EventEmitter<InputEvent> for TextInput {}

impl CaretHost for TextInput {
    fn caret(&mut self) -> &mut CaretBlink {
        &mut self.caret
    }
}

impl TextInput {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_stop(true),
            buffer: TextBuffer::default(),
            placeholder: placeholder.into(),
            last_layout: None,
            last_bounds: None,
            scroll_offset: px(0.0),
            motion: CaretMotion::default(),
            palette: FORGE_DEFAULT,
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
            compact: false,
            fixed_box: None,
            invalid: false,
            caret: CaretBlink::new(),
            committed: SharedString::default(),
            blur_sub: None,
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

    pub fn with_font_size(mut self, size: Pixels) -> Self {
        self.font_size = size;
        self
    }

    #[must_use]
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    #[must_use]
    pub fn fixed_box(mut self, height: Pixels, pad_x: Pixels) -> Self {
        self.fixed_box = Some((height, pad_x));
        self
    }

    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
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
        self.buffer.as_str()
    }

    pub fn set_palette(&mut self, palette: ForgePalette, cx: &mut Context<Self>) {
        self.palette = palette;
        cx.notify();
    }

    pub fn set_static_chrome(&mut self, chrome: Option<(Rgba, Radius)>) {
        self.static_chrome = chrome;
    }

    pub fn set_secure(&mut self, secure: bool, cx: &mut Context<Self>) {
        if self.secure != secure {
            self.secure = secure;
            cx.notify();
        }
    }

    pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        if self.read_only != read_only {
            self.read_only = read_only;
            cx.notify();
        }
    }

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
        let text = text.into();
        self.committed = text.clone();
        self.buffer.reset(text.into());
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set_content("", cx);
    }

    pub fn insert_at_caret(&mut self, text: &str, cx: &mut Context<Self>) {
        self.insert_text(text, cx);
    }

    pub fn restore_committed(&mut self, cx: &mut Context<Self>) {
        let committed = self.committed.clone();
        self.set_content(committed, cx);
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
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

    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        self.committed = self.buffer.text().into();
        cx.emit(InputEvent::Submitted(self.committed.clone()));
    }

    fn on_blur(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.commit_on_leave(cx);
    }

    fn commit_on_leave(&mut self, cx: &mut Context<Self>) {
        if self.buffer.as_str() == self.committed.as_ref() {
            return;
        }
        self.committed = self.buffer.text().into();
        cx.emit(InputEvent::Blurred(self.committed.clone()));
    }

    fn focus_next_field(
        &mut self,
        _: &FocusNextField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_on_leave(cx);
        window.focus_next(cx);
    }

    fn focus_previous_field(
        &mut self,
        _: &FocusPreviousField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_on_leave(cx);
        window.focus_prev(cx);
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(InputEvent::Cancelled);
    }

    fn display_offset(&self, offset: usize) -> usize {
        if self.secure {
            mask_offset(self.buffer.as_str(), offset)
        } else {
            offset
        }
    }

    fn content_offset(&self, display_offset: usize) -> usize {
        if self.secure {
            content_offset_for_mask(self.buffer.as_str(), display_offset)
        } else {
            display_offset
        }
    }
}

impl TextEditor for TextInput {
    fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    fn buffer_mut(&mut self) -> &mut TextBuffer {
        &mut self.buffer
    }

    fn motion(&mut self) -> &mut CaretMotion {
        &mut self.motion
    }

    fn policy(&self) -> EditPolicy {
        EditPolicy {
            read_only: self.read_only,
            single_line: true,
            concealed: self.secure,
        }
    }

    fn offset_for_position(&self, position: Point<Pixels>) -> usize {
        if self.buffer.as_str().is_empty() {
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
            return self.buffer.as_str().len();
        }
        self.content_offset(
            line.closest_index_for_x(position.x - bounds.left() + self.scroll_offset),
        )
    }

    fn offset_for_input_point(&self, point: Point<Pixels>) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let line = self.last_layout.as_ref()?;
        let index = line.index_for_x(point.x - line_point.x + self.scroll_offset)?;
        Some(self.content_offset(index))
    }

    fn bounds_for_offsets(
        &self,
        range: Range<usize>,
        element_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let origin_x = element_bounds.left() - self.scroll_offset;
        Some(Bounds::from_corners(
            point(
                origin_x + line.x_for_index(self.display_offset(range.start)),
                element_bounds.top(),
            ),
            point(
                origin_x + line.x_for_index(self.display_offset(range.end)),
                element_bounds.bottom(),
            ),
        ))
    }
}

forward_input_handler!(TextInput);

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
        let content = SharedString::from(input.buffer.text());
        let selected_range = input.buffer.selected_range().clone();
        let cursor = input.cursor_offset();
        let palette = input.palette;
        let secure = input.secure;
        let mut scroll_offset = input.scroll_offset;
        let style = window.text_style();

        let (display_text, text_color): (SharedString, Hsla) = if content.is_empty() {
            (input.placeholder.clone(), palette.text_muted.into())
        } else if secure {
            (SharedString::from(mask_graphemes(&content)), style.color)
        } else {
            (content.clone(), style.color)
        };

        let display_index = |offset: usize| input.display_offset(offset);

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.buffer.marked_range().filter(|_| !secure) {
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
            && self.input.read(cx).caret.is_lit()
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
        if self.blur_sub.is_none() {
            let handle = self.focus_handle.clone();
            self.blur_sub = Some(cx.on_blur(&handle, window, Self::on_blur));
        }
        let focus = self.focus_handle.clone();
        self.caret.watch(&focus, window, cx);
        let focused = focus.is_focused(window);
        set_caret_blinking(self, focused && window.is_visible(), cx);
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

        let field = div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::focus_next_field))
            .on_action(cx.listener(Self::focus_previous_field))
            .on_action(cx.listener(Self::paste));
        let mut field = with_editing_actions(field, cx)
            .w_full()
            .flex()
            .items_center()
            .overflow_hidden();
        if !self.plain {
            let py = if self.compact {
                px(4.0)
            } else {
                spacing(Spacing::Xs, self.density)
            };
            field = match self.fixed_box {
                Some((height, pad_x)) => field.h(height).px(pad_x),
                None => field.px(spacing(Spacing::Sm, self.density)).py(py),
            };
            field = field
                .bg(surface)
                .border(BORDER_THIN)
                .border_color(border_color)
                .rounded(radius(corner));
        }
        let font_family = if self.mono {
            mono_family()
        } else {
            body_family()
        };
        let mut field = field
            .font_family(font_family)
            .text_size(self.font_size)
            .text_color(text_color)
            .line_height(self.font_size * if self.compact { 1.2 } else { 1.5 });

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
                    .font_family(mono_family())
                    .text_size(self.font_size)
                    .text_color(self.palette.text_faint)
                    .child(prefix),
            );
        }
        field.child(content)
    }
}

#[cfg(test)]
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
    fn a_read_only_input_ignores_every_edit(cx: &mut gpui::TestAppContext) {
        type Edit = fn(&mut TextInput, &mut Window, &mut Context<TextInput>);
        let edits: [(&str, Edit); 7] = [
            ("insert at caret", |input, _window, cx| {
                input.insert_at_caret("%user%", cx)
            }),
            ("type", |input, window, cx| {
                input.replace_text_in_range(None, "x", window, cx)
            }),
            ("compose", |input, window, cx| {
                input.replace_and_mark_text_in_range(None, "x", None, window, cx)
            }),
            ("backspace", |input, window, cx| {
                input.end(&End, window, cx);
                input.backspace(&Backspace, window, cx)
            }),
            ("delete", |input, window, cx| {
                input.home(&Home, window, cx);
                input.delete(&Delete, window, cx)
            }),
            ("cut", |input, window, cx| {
                input.select_all(&SelectAll, window, cx);
                input.cut(&Cut, window, cx)
            }),
            ("paste", |input, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string("x".to_owned()));
                input.paste(&Paste, window, cx)
            }),
        ];
        for (label, edit) in edits {
            let window =
                cx.add_window(|_window, cx| TextInput::new("placeholder", cx).read_only(true));
            let content = window
                .update(cx, |input, window, cx| {
                    input.set_content("alice", cx);
                    edit(input, window, cx);
                    input.content().to_owned()
                })
                .unwrap();

            assert_eq!(content, "alice", "{label}");
        }
    }

    #[gpui::test]
    fn backspace_deletes_the_whole_previous_grapheme_cluster(cx: &mut gpui::TestAppContext) {
        for (before, after) in [("a😀", "a"), ("café", "caf"), ("🇺🇦", ""), ("👨‍👩‍👧", "")]
        {
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
    fn utf16_offsets_map_across_the_astral_plane(cx: &mut gpui::TestAppContext) {
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

    const SEED: &str = "amy";
    const TYPED: &str = "!";

    struct Commits {
        seen: Vec<String>,
        _sub: gpui::Subscription,
    }

    fn commit_label(event: &InputEvent) -> Option<String> {
        match event {
            InputEvent::Submitted(text) => Some(format!("submitted:{text}")),
            InputEvent::Blurred(text) => Some(format!("blurred:{text}")),
            InputEvent::Cancelled => Some("cancelled".to_owned()),
            InputEvent::Changed(_) => None,
        }
    }

    fn commits_while(
        cx: &mut gpui::TestAppContext,
        act: impl FnOnce(&Entity<TextInput>, &mut gpui::VisualTestContext),
    ) -> Vec<String> {
        let (input, vcx) = cx.add_window_view(|_window, cx| TextInput::new("placeholder", cx));
        vcx.update(|window, _cx| window.activate_window());
        let commits = vcx.update(|_window, cx| {
            cx.new(|cx| Commits {
                seen: Vec::new(),
                _sub: cx.subscribe(&input, |this: &mut Commits, _field, event, _cx| {
                    if let Some(label) = commit_label(event) {
                        this.seen.push(label);
                    }
                }),
            })
        });
        vcx.update(|_window, cx| input.update(cx, |field, cx| field.set_content(SEED, cx)));
        vcx.update(|window, cx| input.update(cx, |field, cx| field.focus(window, cx)));
        vcx.run_until_parked();

        act(&input, vcx);
        vcx.run_until_parked();

        vcx.update(|_window, cx| commits.read(cx).seen.clone())
    }

    fn type_more(input: &Entity<TextInput>, vcx: &mut gpui::VisualTestContext) {
        vcx.update(|window, cx| {
            input.update(cx, |field, cx| {
                field.replace_text_in_range(None, TYPED, window, cx);
            });
        });
    }

    fn leave(vcx: &mut gpui::VisualTestContext) {
        vcx.update(|window, cx| window.blur(cx));
        vcx.run_until_parked();
    }

    #[gpui::test]
    fn a_field_nobody_edited_stays_silent_when_focus_leaves(cx: &mut gpui::TestAppContext) {
        let seen = commits_while(cx, |_input, vcx| leave(vcx));

        assert!(seen.is_empty(), "expected no commit, saw {seen:?}");
    }

    #[gpui::test]
    fn an_edited_field_commits_once_and_the_blur_after_that_is_silent(
        cx: &mut gpui::TestAppContext,
    ) {
        let seen = commits_while(cx, |input, vcx| {
            type_more(input, vcx);
            leave(vcx);
            vcx.update(|window, cx| input.update(cx, |field, cx| field.focus(window, cx)));
            vcx.run_until_parked();
            leave(vcx);
        });

        assert_eq!(seen, ["blurred:amy!"]);
    }

    #[gpui::test]
    fn enter_commits_so_the_blur_that_follows_it_emits_nothing(cx: &mut gpui::TestAppContext) {
        let seen = commits_while(cx, |input, vcx| {
            type_more(input, vcx);
            vcx.update(|window, cx| {
                input.update(cx, |field, cx| field.submit(&Submit, window, cx));
            });
            leave(vcx);
        });

        assert_eq!(seen, ["submitted:amy!"]);
    }

    #[gpui::test]
    fn set_content_moves_the_baseline_so_the_text_it_installed_never_commits(
        cx: &mut gpui::TestAppContext,
    ) {
        let seen = commits_while(cx, |input, vcx| {
            type_more(input, vcx);
            vcx.update(|_window, cx| input.update(cx, |field, cx| field.set_content("alan", cx)));
            leave(vcx);
        });

        assert!(seen.is_empty(), "expected no commit, saw {seen:?}");
    }

    #[gpui::test]
    fn restore_committed_puts_back_the_last_commit_without_a_commit_of_its_own(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut restored = String::new();
        let seen = commits_while(cx, |input, vcx| {
            type_more(input, vcx);
            vcx.update(|_window, cx| input.update(cx, |field, cx| field.restore_committed(cx)));
            restored = vcx.update(|_window, cx| input.read(cx).content().to_string());
            leave(vcx);
        });

        assert_eq!(restored, SEED);
        assert!(seen.is_empty(), "expected no commit, saw {seen:?}");
    }

    fn press(vcx: &mut gpui::VisualTestContext, keys: &str) {
        vcx.update(|_window, cx| bind_text_input_keys(cx));
        vcx.simulate_keystrokes(keys);
        vcx.run_until_parked();
    }

    #[gpui::test]
    fn tab_commits_an_edited_field_even_when_it_is_the_only_tab_stop(
        cx: &mut gpui::TestAppContext,
    ) {
        for key in ["tab", "shift-tab"] {
            let seen = commits_while(cx, |input, vcx| {
                type_more(input, vcx);
                press(vcx, key);
                leave(vcx);
            });

            assert_eq!(seen, ["blurred:amy!"], "{key}");
        }
    }

    struct TwoFields {
        first: Entity<TextInput>,
        second: Entity<TextInput>,
    }

    impl Render for TwoFields {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().child(self.first.clone()).child(self.second.clone())
        }
    }

    struct FocusWalk {
        first_commits: Vec<String>,
        focused_first: Vec<bool>,
    }

    fn walk_two_fields(
        cx: &mut gpui::TestAppContext,
        edit_first: bool,
        keys: &[&str],
    ) -> FocusWalk {
        let (form, vcx) = cx.add_window_view(|_window, cx| TwoFields {
            first: cx.new(|cx| TextInput::new("first", cx)),
            second: cx.new(|cx| TextInput::new("second", cx)),
        });
        vcx.update(|window, _cx| window.activate_window());
        let (first, second) =
            vcx.update(|_window, cx| (form.read(cx).first.clone(), form.read(cx).second.clone()));
        let commits = vcx.update(|_window, cx| {
            cx.new(|cx| Commits {
                seen: Vec::new(),
                _sub: cx.subscribe(&first, |this: &mut Commits, _field, event, _cx| {
                    if let Some(label) = commit_label(event) {
                        this.seen.push(label);
                    }
                }),
            })
        });
        vcx.update(|_window, cx| first.update(cx, |field, cx| field.set_content(SEED, cx)));
        vcx.update(|window, cx| first.update(cx, |field, cx| field.focus(window, cx)));
        vcx.run_until_parked();
        if edit_first {
            type_more(&first, vcx);
        }

        let mut focused_first = Vec::new();
        for key in keys {
            press(vcx, key);
            let (on_first, on_second) = vcx.update(|window, cx| {
                (
                    first.read(cx).focus_handle(cx).is_focused(window),
                    second.read(cx).focus_handle(cx).is_focused(window),
                )
            });
            assert_ne!(
                on_first, on_second,
                "exactly one field holds focus after {key}"
            );
            focused_first.push(on_first);
        }
        FocusWalk {
            first_commits: vcx.update(|_window, cx| commits.read(cx).seen.clone()),
            focused_first,
        }
    }

    #[gpui::test]
    fn tab_and_shift_tab_move_focus_between_fields(cx: &mut gpui::TestAppContext) {
        let walk = walk_two_fields(cx, false, &["tab", "shift-tab"]);

        assert_eq!(walk.focused_first, [false, true]);
    }

    #[gpui::test]
    fn tab_away_commits_the_edit_once_and_later_round_trips_stay_silent(
        cx: &mut gpui::TestAppContext,
    ) {
        let walk = walk_two_fields(cx, true, &["tab", "shift-tab", "tab"]);

        assert_eq!(walk.first_commits, ["blurred:amy!"]);
    }

    #[gpui::test]
    fn tab_away_from_an_unedited_field_emits_nothing(cx: &mut gpui::TestAppContext) {
        let walk = walk_two_fields(cx, false, &["tab"]);

        assert!(
            walk.first_commits.is_empty(),
            "expected no commit, saw {:?}",
            walk.first_commits
        );
    }

    #[gpui::test]
    fn insert_at_caret_lands_at_the_cursor_and_advances_it(cx: &mut gpui::TestAppContext) {
        let (content, cursor) = with_input(cx, "helo", |input, window, cx| {
            input.home(&Home, window, cx);
            input.right(&Right, window, cx);
            input.right(&Right, window, cx);
            input.insert_at_caret("%x%", cx);
            (input.content().to_string(), input.cursor_offset())
        });
        assert_eq!((content.as_str(), cursor), ("he%x%lo", 5));
    }

    #[gpui::test]
    fn insert_at_caret_replaces_the_selection_or_an_open_composition(
        cx: &mut gpui::TestAppContext,
    ) {
        type Prepare = fn(&mut TextInput, &mut Window, &mut Context<TextInput>);
        let cases: [(&str, Prepare, &str); 2] = [
            (
                "selection",
                |input, window, cx| {
                    input.home(&Home, window, cx);
                    input.select_right(&SelectRight, window, cx);
                    input.select_right(&SelectRight, window, cx);
                },
                "%x%llo",
            ),
            (
                "composition",
                |input, window, cx| {
                    input.replace_and_mark_text_in_range(None, "ж", None, window, cx);
                },
                "hello%x%",
            ),
        ];
        for (case, prepare, expected) in cases {
            let content = with_input(cx, "hello", |input, window, cx| {
                prepare(input, window, cx);
                input.insert_at_caret("%x%", cx);
                input.content().to_string()
            });
            assert_eq!(content, expected, "{case}");
        }
    }

    #[gpui::test]
    fn insert_at_caret_and_paste_flatten_newlines_into_spaces(cx: &mut gpui::TestAppContext) {
        type Insert = fn(&mut TextInput, &mut Window, &mut Context<TextInput>);
        let inserts: [(&str, Insert); 2] = [
            ("insert at caret", |input, _window, cx| {
                input.insert_at_caret("a\nb\n", cx)
            }),
            ("paste", |input, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string("a\nb\n".to_owned()));
                input.paste(&Paste, window, cx)
            }),
        ];
        for (label, insert) in inserts {
            let content = with_input(cx, "", |input, window, cx| {
                insert(input, window, cx);
                input.content().to_string()
            });
            assert_eq!(content, "a b ", "{label}");
        }
    }

    #[gpui::test]
    fn a_secure_input_never_puts_its_text_on_the_clipboard(cx: &mut gpui::TestAppContext) {
        type Take = fn(&mut TextInput, &mut Window, &mut Context<TextInput>);
        let copy: Take = |input, window, cx| input.copy(&Copy, window, cx);
        let cut: Take = |input, window, cx| input.cut(&Cut, window, cx);
        for (secure, label, take, clipboard, content) in [
            (false, "copy", copy, "hunter2", "hunter2"),
            (false, "cut", cut, "hunter2", ""),
            (true, "copy", copy, "untouched", "hunter2"),
            (true, "cut", cut, "untouched", ""),
        ] {
            let window =
                cx.add_window(|_window, cx| TextInput::new("placeholder", cx).secure(secure));
            let seen = window
                .update(cx, |input, window, cx| {
                    input.set_content("hunter2", cx);
                    cx.write_to_clipboard(ClipboardItem::new_string("untouched".to_owned()));
                    input.select_all(&SelectAll, window, cx);
                    take(input, window, cx);
                    let pasted = cx
                        .read_from_clipboard()
                        .and_then(|item| item.text())
                        .unwrap_or_default();
                    (pasted, input.content().to_owned())
                })
                .unwrap();
            assert_eq!(
                seen,
                (clipboard.to_owned(), content.to_owned()),
                "secure {secure} {label}"
            );
        }
    }

    fn with_secure_input<R>(
        cx: &mut gpui::TestAppContext,
        secure: bool,
        f: impl FnOnce(&mut TextInput, &mut Window, &mut Context<TextInput>) -> R,
    ) -> R {
        let window = cx.add_window(|_window, cx| TextInput::new("placeholder", cx).secure(secure));
        window
            .update(cx, |input, window, cx| {
                input.set_content("pa\u{1F600}ss", cx);
                f(input, window, cx)
            })
            .unwrap()
    }

    #[gpui::test]
    fn a_secure_input_reveals_no_text_to_the_input_method(cx: &mut gpui::TestAppContext) {
        type Read = (Option<String>, Option<Range<usize>>);
        let plain: Vec<Read> = vec![
            (Some("pa\u{1F600}ss".to_owned()), Some(0..6)),
            (Some("a".to_owned()), Some(1..2)),
            (Some(String::new()), Some(6..6)),
        ];
        let hidden: Vec<Read> = vec![(None, None); 3];
        for (secure, want) in [(false, plain), (true, hidden)] {
            let got = with_secure_input(cx, secure, |input, window, cx| {
                [0..6, 1..2, 9..12]
                    .into_iter()
                    .map(|range| {
                        let mut actual = None;
                        let text = input.text_for_range(range, &mut actual, window, cx);
                        (text, actual)
                    })
                    .collect::<Vec<Read>>()
            });
            assert_eq!(got, want, "secure {secure}");
        }
    }

    #[gpui::test]
    fn a_secure_input_ignores_compositions_but_accepts_committed_text(
        cx: &mut gpui::TestAppContext,
    ) {
        for (secure, composed, marked, typed) in [
            (
                false,
                "pa\u{1F600}ss\u{304B}",
                true,
                "pa\u{1F600}ss\u{304B}x",
            ),
            (true, "pa\u{1F600}ss", false, "pa\u{1F600}ssx"),
        ] {
            let got = with_secure_input(cx, secure, |input, window, cx| {
                input.replace_and_mark_text_in_range(None, "\u{304B}", Some(1..1), window, cx);
                let after_compose = (
                    input.content().to_owned(),
                    input.marked_text_range(window, cx).is_some(),
                );
                input.unmark_text(window, cx);
                input.replace_text_in_range(None, "x", window, cx);
                (after_compose, input.content().to_owned())
            });
            assert_eq!(
                got,
                ((composed.to_owned(), marked), typed.to_owned()),
                "secure {secure}"
            );
        }
    }

    #[gpui::test]
    fn locking_an_input_read_only_blocks_undo_of_an_earlier_edit(cx: &mut gpui::TestAppContext) {
        let content = with_input(cx, "alice", |input, window, cx| {
            input.replace_text_in_range(None, "!", window, cx);
            input.set_read_only(true, cx);
            input.undo(&Undo, window, cx);
            input.content().to_owned()
        });
        assert_eq!(content, "alice!");
    }

    #[gpui::test]
    fn insert_at_caret_announces_the_new_content(cx: &mut gpui::TestAppContext) {
        let window = cx.add_window(|_window, cx| TextInput::new("placeholder", cx));
        let input = window.root(cx).unwrap();
        let heard = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = std::rc::Rc::clone(&heard);
        let _subscription = cx.update(|cx| {
            cx.subscribe(&input, move |_, event: &InputEvent, _| {
                if let InputEvent::Changed(text) = event {
                    sink.borrow_mut().push(text.to_string());
                }
            })
        });

        window
            .update(cx, |input, _window, cx| input.insert_at_caret("%x%", cx))
            .unwrap();

        assert_eq!(*heard.borrow(), vec!["%x%".to_string()]);
    }
}
