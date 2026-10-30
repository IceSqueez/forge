mod element;
mod layout;
pub(crate) mod lines;

use std::ops::{Range, RangeInclusive};

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, EventEmitter, FocusHandle, Focusable,
    KeyBinding, Pixels, Point, ScrollEvent, SharedString, Subscription, Window, actions, div,
    point, prelude::*, px,
};

use crate::caret_blink::{CaretBlink, CaretHost, set_caret_blinking};
use crate::highlight::Language;
use crate::palette::{FORGE_DEFAULT, ForgePalette};
use crate::presentation::ActivePresentation;
use crate::text_buffer::TextBuffer;
use crate::text_editing::*;
use crate::text_input::{FocusNextField, FocusPreviousField, InputEvent};
use crate::tokens::{BORDER_THIN, Density, FONT_XS, Radius, Spacing, mono_family, radius, spacing};

use element::{CodeEditorElement, PAD_Y};
use layout::{Geometry, Lines};

const KEY_CONTEXT: &str = "ForgeCodeEditor";
const INDENT_UNIT: &str = "  ";
const SAVE_CHORD: &str = "secondary-s";
const LEAVE_CHORD: &str = "escape";
const COMPACT_FIELD_ROWS: RangeInclusive<usize> = 1..=4;

actions!(forge_code_editor, [Indent, Outdent, SaveCode, LeaveEditor]);

pub fn bind_code_editor_keys(cx: &mut App) {
    let mut bindings = editing_key_bindings(KEY_CONTEXT);
    bindings.push(KeyBinding::new("tab", Indent, Some(KEY_CONTEXT)));
    bindings.push(KeyBinding::new("shift-tab", Outdent, Some(KEY_CONTEXT)));
    bindings.push(KeyBinding::new(SAVE_CHORD, SaveCode, Some(KEY_CONTEXT)));
    bindings.push(KeyBinding::new(LEAVE_CHORD, LeaveEditor, Some(KEY_CONTEXT)));
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
    field_rows: Option<RangeInclusive<usize>>,
    compact: bool,
    density: Density,
    rendered_rows: usize,
    last_bounds: Option<Bounds<Pixels>>,
    text_left: Pixels,
    scroll_offset: Pixels,
    follow_caret: bool,
    motion: CaretMotion,
    caret: CaretBlink,
    committed: SharedString,
    blur_sub: Option<Subscription>,
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
            focus_handle: cx.focus_handle().tab_stop(true),
            buffer: TextBuffer::default(),
            lines: Lines::new(language),
            geometry: Geometry::default(),
            placeholder: placeholder.into(),
            palette: FORGE_DEFAULT,
            font_size: FONT_XS,
            line_height: FONT_XS * LINE_HEIGHT_RATIO,
            gutter_marks: Vec::new(),
            field_rows: None,
            compact: false,
            density: Density::default(),
            rendered_rows: 0,
            last_bounds: None,
            text_left: px(0.0),
            scroll_offset: px(0.0),
            follow_caret: false,
            motion: CaretMotion::default(),
            caret: CaretBlink::new(),
            committed: SharedString::default(),
            blur_sub: None,
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

    pub fn with_field_rows(mut self, rows: RangeInclusive<usize>) -> Self {
        self.field_rows = Some(rows);
        self
    }

    pub fn compact_field(mut self) -> Self {
        self.field_rows = Some(COMPACT_FIELD_ROWS);
        self.compact = true;
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
        let text = text.into();
        self.committed = text.clone();
        self.buffer.reset(text.into());
        self.buffer.move_to(0);
        self.lines.sync(self.buffer.text());
        self.geometry.invalidate();
        self.motion.goal_x = None;
        self.scroll_offset = px(0.0);
        self.follow_caret = false;
        cx.notify();
    }

    pub fn replace_content_in_place(&mut self, text: &str, cx: &mut Context<Self>) {
        if text == self.buffer.as_str() {
            return;
        }
        self.motion.goal_x = None;
        self.buffer.replace_all_keeping_caret_line(text);
        self.edited(cx);
        self.follow_caret = false;
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

    pub fn insert_at_caret(&mut self, text: &str, cx: &mut Context<Self>) {
        self.insert_text(text, cx);
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    fn pad_y(&self) -> Pixels {
        if self.compact {
            spacing(Spacing::Xs, self.density)
        } else {
            PAD_Y
        }
    }

    fn content_rows(&self) -> usize {
        if self.compact {
            self.geometry
                .row_count(&self.lines)
                .unwrap_or_else(|| self.lines.len())
        } else {
            self.lines.len()
        }
    }

    fn rows_outgrew_render(&self) -> bool {
        self.compact && self.content_rows() != self.rendered_rows
    }

    fn field_height(&self, rows: &RangeInclusive<usize>) -> Pixels {
        let shown = self.content_rows().max(*rows.start()).min(*rows.end());
        let pad_y = self.pad_y();
        self.line_height * shown as f32 + pad_y + pad_y + BORDER_THIN + BORDER_THIN
    }

    fn gutter_corner_radius(&self) -> Pixels {
        match self.field_rows {
            Some(_) => radius(Radius::Md) - BORDER_THIN,
            None => px(0.0),
        }
    }

    fn insert_newline(&mut self, _: &InsertNewline, _: &mut Window, cx: &mut Context<Self>) {
        if !self.compact {
            self.edit(TextBuffer::insert_newline_keeping_indent, cx);
        }
    }

    fn indent(&mut self, _: &Indent, window: &mut Window, cx: &mut Context<Self>) {
        if self.compact {
            self.focus_handle
                .dispatch_action(&FocusNextField, window, cx);
            return;
        }
        self.edit(|buffer| buffer.indent(INDENT_UNIT), cx);
    }

    fn outdent(&mut self, _: &Outdent, window: &mut Window, cx: &mut Context<Self>) {
        if self.compact {
            self.focus_handle
                .dispatch_action(&FocusPreviousField, window, cx);
            return;
        }
        self.edit(|buffer| buffer.outdent(INDENT_UNIT), cx);
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

    fn cancel_composition(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.buffer.discard_marked() {
            return false;
        }
        self.motion.goal_x = None;
        self.edited(cx);
        true
    }

    fn on_blur(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.committed.as_ref() == self.buffer.as_str() {
            return;
        }
        self.committed = SharedString::from(self.buffer.text().clone());
        cx.emit(InputEvent::Blurred(self.committed.clone()));
    }

    fn on_scroll_wheel(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
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
        let pad_y = self.pad_y();
        (self.geometry.total() + pad_y + pad_y - view_height).max(px(0.0))
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
}

impl TextEditor for CodeEditor {
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

    fn sync_after_edit(&mut self) {
        if self.lines.sync(self.buffer.text()) {
            self.geometry.invalidate();
        }
    }

    fn offset_for_position(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        let x = position.x - bounds.left() - self.text_left;
        let y = (position.y - bounds.top() - self.pad_y() + self.scroll_offset).max(px(0.0));
        self.geometry.offset_for_point(&self.lines, point(x, y))
    }

    fn bounds_for_offsets(
        &self,
        range: Range<usize>,
        element_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        let line_height = self.geometry.metrics()?.line_height;
        let start = self.geometry.point_for_offset(&self.lines, range.start)?;
        let origin = point(
            element_bounds.left() + self.text_left + start.x,
            element_bounds.top() + self.pad_y() + start.y - self.scroll_offset,
        );
        Some(Bounds::from_corners(
            origin,
            point(origin.x, origin.y + line_height),
        ))
    }

    fn vertical_target(
        &mut self,
        below: bool,
        goal_x: Option<Pixels>,
        window: &mut Window,
    ) -> Option<(Pixels, usize)> {
        let cursor = self.buffer.cursor();
        self.shape_around(self.lines.line_index_at(cursor), window);
        let line_height = self.geometry.metrics()?.line_height;
        let caret = self.geometry.point_for_offset(&self.lines, cursor)?;
        let goal_x = goal_x.unwrap_or(caret.x);
        let probe_y = vertical_probe_y(caret.y, line_height, below, self.geometry.total());
        let target = self
            .geometry
            .offset_for_point(&self.lines, point(goal_x, probe_y));
        Some((goal_x, target))
    }
}

forward_input_handler!(CodeEditor);

impl Focusable for CodeEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CodeEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.blur_sub.is_none() {
            let handle = self.focus_handle.clone();
            self.blur_sub = Some(cx.on_blur(&handle, window, Self::on_blur));
        }
        self.density = cx.density();
        let focus = self.focus_handle.clone();
        self.caret.watch(&focus, window, cx);
        let focused = focus.is_focused(window);
        set_caret_blinking(self, focused && window.is_visible(), cx);
        let editor_handle = cx.entity().downgrade();
        let leave_focus = focus.clone();
        let stuck_focus = focus.clone();

        let editor = div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::insert_newline))
            .on_action(cx.listener(Self::indent))
            .on_action(cx.listener(Self::outdent))
            .on_action(cx.listener(Self::paste))
            .on_action(move |_: &LeaveEditor, window, cx| {
                let composing = editor_handle
                    .update(cx, |editor, cx| editor.cancel_composition(cx))
                    .unwrap_or(false);
                if composing {
                    return;
                }
                leave_focus.dispatch_action(&FocusNextField, window, cx);
                cx.propagate();
            })
            .on_action(move |_: &FocusNextField, window, cx| {
                window.focus_next(cx);
                if stuck_focus.is_focused(window) {
                    cx.propagate();
                }
            })
            .on_action(|_: &FocusPreviousField, window, cx| window.focus_prev(cx))
            .on_scroll(cx.listener(Self::on_scroll_wheel));
        let editor = with_multiline_actions(with_editing_actions(editor, cx), cx)
            .w_full()
            .overflow_hidden()
            .font_family(mono_family())
            .text_size(self.font_size)
            .text_color(self.palette.text_primary)
            .line_height(self.line_height);
        self.rendered_rows = self.content_rows();
        let editor = match self.field_rows.as_ref().map(|rows| self.field_height(rows)) {
            Some(height) => editor
                .h(height)
                .flex_none()
                .bg(self.palette.shell)
                .border(BORDER_THIN)
                .border_color(if focused {
                    self.palette.border_active
                } else {
                    self.palette.border_input
                })
                .rounded(radius(Radius::Md)),
            None => editor.flex_1().min_h(px(0.0)).bg(self.palette.base),
        };
        editor.child(CodeEditorElement::new(cx.entity()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::install_presentation;
    use gpui::{Entity, TestAppContext, VisualTestContext};

    const LOADED: &str = "let a = 1;\nlet b=2;";
    const TYPED: &str = "!";

    struct Heard {
        seen: Vec<String>,
        _sub: Subscription,
    }

    fn label(event: &InputEvent) -> String {
        match event {
            InputEvent::Changed(text) => format!("changed:{text}"),
            InputEvent::Submitted(text) => format!("submitted:{text}"),
            InputEvent::Blurred(text) => format!("blurred:{text}"),
            InputEvent::Cancelled => "cancelled".to_owned(),
        }
    }

    fn events_while(
        cx: &mut TestAppContext,
        act: impl FnOnce(&Entity<CodeEditor>, &mut VisualTestContext),
    ) -> Vec<String> {
        install_presentation(cx);
        let (editor, vcx) =
            cx.add_window_view(|_window, cx| CodeEditor::new(Language::Rhai, "", cx));
        vcx.update(|window, _cx| window.activate_window());
        vcx.update(|_window, cx| editor.update(cx, |editor, cx| editor.set_content(LOADED, cx)));
        let heard = vcx.update(|_window, cx| {
            cx.new(|cx| Heard {
                seen: Vec::new(),
                _sub: cx.subscribe(&editor, |this: &mut Heard, _editor, event, _cx| {
                    this.seen.push(label(event));
                }),
            })
        });
        vcx.update(|window, cx| editor.update(cx, |editor, cx| editor.focus(window, cx)));
        vcx.run_until_parked();

        act(&editor, vcx);
        vcx.run_until_parked();

        vcx.update(|_window, cx| heard.read(cx).seen.clone())
    }

    fn type_more(editor: &Entity<CodeEditor>, vcx: &mut VisualTestContext) {
        vcx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.replace_text_in_range(None, TYPED, window, cx);
            });
        });
    }

    fn leave(vcx: &mut VisualTestContext) {
        vcx.update(|window, cx| window.blur(cx));
        vcx.run_until_parked();
    }

    fn refocus(editor: &Entity<CodeEditor>, vcx: &mut VisualTestContext) {
        vcx.update(|window, cx| editor.update(cx, |editor, cx| editor.focus(window, cx)));
        vcx.run_until_parked();
    }

    fn blurs(seen: &[String]) -> Vec<&str> {
        seen.iter()
            .map(String::as_str)
            .filter(|label| label.starts_with("blurred:"))
            .collect()
    }

    #[gpui::test]
    fn leaving_an_unedited_editor_commits_nothing(cx: &mut TestAppContext) {
        let seen = events_while(cx, |_editor, vcx| leave(vcx));

        assert!(seen.is_empty(), "expected silence, saw {seen:?}");
    }

    #[gpui::test]
    fn leaving_an_edited_editor_commits_once_and_the_next_blur_is_silent(cx: &mut TestAppContext) {
        let seen = events_while(cx, |editor, vcx| {
            type_more(editor, vcx);
            leave(vcx);
            refocus(editor, vcx);
            leave(vcx);
        });

        assert_eq!(blurs(&seen), [format!("blurred:{TYPED}{LOADED}")]);
    }

    #[gpui::test]
    fn edits_undone_back_to_the_loaded_text_commit_nothing_on_blur(cx: &mut TestAppContext) {
        let seen = events_while(cx, |editor, vcx| {
            type_more(editor, vcx);
            vcx.update(|window, cx| {
                editor.update(cx, |editor, cx| editor.undo(&Undo, window, cx));
            });
            leave(vcx);
        });

        assert!(blurs(&seen).is_empty(), "expected no commit, saw {seen:?}");
    }

    #[gpui::test]
    fn set_content_moves_the_baseline_so_the_text_it_installed_never_commits(
        cx: &mut TestAppContext,
    ) {
        let seen = events_while(cx, |editor, vcx| {
            type_more(editor, vcx);
            vcx.update(|_window, cx| {
                editor.update(cx, |editor, cx| editor.set_content("print(1);", cx));
            });
            leave(vcx);
        });

        assert!(blurs(&seen).is_empty(), "expected no commit, saw {seen:?}");
    }

    #[gpui::test]
    fn replacing_in_place_with_identical_text_announces_nothing_and_adds_no_undo_step(
        cx: &mut TestAppContext,
    ) {
        let mut undo_left = None;
        let seen = events_while(cx, |editor, vcx| {
            vcx.update(|_window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.replace_content_in_place(LOADED, cx);
                    undo_left = Some(editor.buffer.undo());
                });
            });
        });

        assert_eq!((seen, undo_left), (Vec::new(), Some(false)));
    }

    #[gpui::test]
    fn replacing_in_place_announces_the_new_text_and_commits_it_on_blur(cx: &mut TestAppContext) {
        const FORMATTED: &str = "let a = 1;\nlet b = 2;";
        let seen = events_while(cx, |editor, vcx| {
            vcx.update(|_window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.replace_content_in_place(FORMATTED, cx)
                });
            });
            leave(vcx);
        });

        assert_eq!(
            seen,
            [
                format!("changed:{FORMATTED}"),
                format!("blurred:{FORMATTED}")
            ]
        );
    }

    #[gpui::test]
    fn replacing_in_place_keeps_the_caret_line_and_does_not_scroll_to_it(cx: &mut TestAppContext) {
        install_presentation(cx);
        let (editor, vcx) =
            cx.add_window_view(|_window, cx| CodeEditor::new(Language::Rhai, "", cx));
        let kept = vcx.update(|_window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_content(LOADED, cx);
                editor.buffer.move_to(LOADED.len());
                editor.replace_content_in_place("let a = 1;\nlet b = 2;", cx);
                (editor.buffer.cursor(), editor.follow_caret)
            })
        });

        assert_eq!(kept, (19, false));
    }

    const DIALOG_CONTEXT: &str = "CodeEditorTestDialog";
    const SHELL_FALLBACK: &str = "next-field";
    const DISMISSED: &str = "dismissed";
    const SAVED: &str = "saved";

    actions!(code_editor_tests, [Dismiss]);

    #[derive(Clone, Copy)]
    struct Surroundings {
        neighbour: bool,
        dialog: bool,
        compact: bool,
    }

    struct Host {
        editor: Entity<CodeEditor>,
        neighbour: FocusHandle,
        surroundings: Surroundings,
        heard: Vec<&'static str>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let mut content = div()
                .size_full()
                .on_action(cx.listener(|this, _: &SaveCode, _, _| this.heard.push(SAVED)))
                .on_action(
                    cx.listener(|this, _: &FocusNextField, _, _| this.heard.push(SHELL_FALLBACK)),
                )
                .child(self.editor.clone());
            if self.surroundings.neighbour {
                content = content.child(div().track_focus(&self.neighbour).child("next"));
            }
            let mut root = div().size_full();
            if self.surroundings.dialog {
                root = root
                    .key_context(DIALOG_CONTEXT)
                    .on_action(cx.listener(|this, _: &Dismiss, _, _| this.heard.push(DISMISSED)));
            }
            root.child(content)
        }
    }

    struct Hosted {
        host: Entity<Host>,
        editor: Entity<CodeEditor>,
    }

    impl Hosted {
        fn bind_keys(cx: &mut TestAppContext) {
            cx.update(|cx| {
                bind_code_editor_keys(cx);
                cx.bind_keys([KeyBinding::new(LEAVE_CHORD, Dismiss, Some(DIALOG_CONTEXT))]);
            });
        }

        fn mount(
            cx: &mut TestAppContext,
            surroundings: Surroundings,
        ) -> (Self, &mut VisualTestContext) {
            install_presentation(cx);
            let (host, vcx) = cx.add_window_view(|_window, cx| Host {
                editor: cx.new(|cx| {
                    let editor = CodeEditor::new(Language::Rhai, "", cx);
                    if surroundings.compact {
                        editor.compact_field()
                    } else {
                        editor
                    }
                }),
                neighbour: cx.focus_handle().tab_stop(true),
                surroundings,
                heard: Vec::new(),
            });
            vcx.update(|window, _cx| window.activate_window());
            let editor = vcx.update(|_window, cx| host.read(cx).editor.clone());
            vcx.update(|_window, cx| {
                editor.update(cx, |editor, cx| editor.set_content(LOADED, cx))
            });
            vcx.update(|window, cx| editor.update(cx, |editor, cx| editor.focus(window, cx)));
            vcx.run_until_parked();
            (Self { host, editor }, vcx)
        }

        fn compose(&self, vcx: &mut VisualTestContext, preedit: &str) {
            vcx.update(|window, cx| {
                self.editor.update(cx, |editor, cx| {
                    editor.replace_and_mark_text_in_range(None, preedit, None, window, cx);
                });
            });
        }

        fn heard(&self, vcx: &mut VisualTestContext) -> Vec<&'static str> {
            vcx.update(|_window, cx| self.host.read(cx).heard.clone())
        }

        fn content(&self, vcx: &mut VisualTestContext) -> String {
            vcx.update(|_window, cx| self.editor.read(cx).content().to_owned())
        }

        fn focus_on(&self, vcx: &mut VisualTestContext) -> (bool, bool) {
            vcx.update(|window, cx| {
                let host = self.host.read(cx);
                (
                    self.editor.read(cx).focus_handle.is_focused(window),
                    host.neighbour.is_focused(window),
                )
            })
        }
    }

    #[gpui::test]
    fn escape_during_a_composition_cancels_only_the_composition(cx: &mut TestAppContext) {
        let surroundings = Surroundings {
            neighbour: true,
            dialog: true,
            compact: false,
        };
        Hosted::bind_keys(cx);
        let (hosted, vcx) = Hosted::mount(cx, surroundings);
        hosted.compose(vcx, "ні");

        vcx.simulate_keystrokes(LEAVE_CHORD);

        assert_eq!(
            (hosted.content(vcx), hosted.focus_on(vcx), hosted.heard(vcx)),
            (LOADED.to_owned(), (true, false), Vec::new())
        );
    }

    #[gpui::test]
    fn escape_outside_a_composition_leaves_the_editor_and_lets_an_enclosing_escape_run(
        cx: &mut TestAppContext,
    ) {
        Hosted::bind_keys(cx);
        for (neighbour, dialog, focus, heard) in [
            (true, false, (false, true), Vec::new()),
            (false, false, (true, false), vec![SHELL_FALLBACK]),
            (true, true, (false, true), vec![DISMISSED]),
        ] {
            let surroundings = Surroundings {
                neighbour,
                dialog,
                compact: false,
            };
            let (hosted, vcx) = Hosted::mount(cx, surroundings);

            vcx.simulate_keystrokes(LEAVE_CHORD);

            assert_eq!(
                (hosted.content(vcx), hosted.focus_on(vcx), hosted.heard(vcx)),
                (LOADED.to_owned(), focus, heard),
                "neighbour {neighbour}, dialog {dialog}"
            );
        }
    }

    #[gpui::test]
    fn the_save_chord_asks_the_enclosing_view_to_save_without_typing(cx: &mut TestAppContext) {
        let surroundings = Surroundings {
            neighbour: false,
            dialog: false,
            compact: false,
        };
        Hosted::bind_keys(cx);
        let (hosted, vcx) = Hosted::mount(cx, surroundings);

        vcx.simulate_keystrokes(SAVE_CHORD);

        assert_eq!(
            (hosted.content(vcx), hosted.heard(vcx)),
            (LOADED.to_owned(), vec![SAVED])
        );
    }

    #[gpui::test]
    fn a_field_shows_its_line_count_clamped_to_the_row_range(cx: &mut TestAppContext) {
        const LINE: Pixels = px(10.0);
        let chrome = PAD_Y + PAD_Y + BORDER_THIN + BORDER_THIN;
        install_presentation(cx);
        let (editor, vcx) = cx.add_window_view(|_window, cx| {
            CodeEditor::new(Language::Rhai, "", cx)
                .with_line_height(LINE)
                .with_field_rows(3..=6)
        });

        for (lines, shown) in [
            (1, 3.0),
            (2, 3.0),
            (3, 3.0),
            (4, 4.0),
            (5, 5.0),
            (6, 6.0),
            (7, 6.0),
            (40, 6.0),
        ] {
            let text = vec!["x"; lines].join("\n");
            let height = vcx.update(|_window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.set_content(text, cx);
                    let rows = editor.field_rows.clone().unwrap_or(0..=0);
                    editor.field_height(&rows)
                })
            });
            assert_eq!(height, LINE * shown + chrome, "{lines} lines");
        }
    }

    fn mount_beside_a_neighbour(
        cx: &mut TestAppContext,
        compact: bool,
    ) -> (Hosted, &mut VisualTestContext) {
        let surroundings = Surroundings {
            neighbour: true,
            dialog: false,
            compact,
        };
        Hosted::mount(cx, surroundings)
    }

    #[gpui::test]
    fn enter_breaks_the_line_in_a_script_editor_but_not_in_a_compact_field(
        cx: &mut TestAppContext,
    ) {
        Hosted::bind_keys(cx);
        for (compact, expected) in [(false, format!("\n{LOADED}")), (true, LOADED.to_owned())] {
            let (hosted, vcx) = mount_beside_a_neighbour(cx, compact);

            vcx.simulate_keystrokes("enter");

            assert_eq!(
                (hosted.content(vcx), hosted.focus_on(vcx)),
                (expected, (true, false)),
                "compact {compact}"
            );
        }
    }

    #[gpui::test]
    fn tab_in_a_compact_field_moves_focus_instead_of_indenting(cx: &mut TestAppContext) {
        Hosted::bind_keys(cx);
        for (keys, compact, content, focus) in [
            (
                "tab",
                false,
                format!("{INDENT_UNIT}{LOADED}"),
                (true, false),
            ),
            ("tab", true, LOADED.to_owned(), (false, true)),
            ("shift-tab", true, LOADED.to_owned(), (false, true)),
        ] {
            let (hosted, vcx) = mount_beside_a_neighbour(cx, compact);

            vcx.simulate_keystrokes(keys);

            assert_eq!(
                (hosted.content(vcx), hosted.focus_on(vcx)),
                (content, focus),
                "{keys}, compact {compact}"
            );
        }
    }

    #[gpui::test]
    fn a_compact_field_keeps_pasted_lines_and_grows_with_its_rows_up_to_four(
        cx: &mut TestAppContext,
    ) {
        const LINE: Pixels = px(10.0);
        let lines = |count: usize| vec!["x"; count].join("\n");
        for (pasted, shown) in [
            (lines(1), 1.0),
            (lines(2), 2.0),
            (lines(4), 4.0),
            (lines(5), 4.0),
            (lines(12), 4.0),
            ("x".repeat(2_000), 4.0),
        ] {
            install_presentation(cx);
            let (editor, vcx) = cx.add_window_view(|_window, cx| {
                CodeEditor::new(Language::Rhai, "", cx)
                    .with_line_height(LINE)
                    .compact_field()
            });
            vcx.update(|_window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(pasted.clone()))
            });

            vcx.update(|window, cx| {
                editor.update(cx, |editor, cx| editor.paste(&Paste, window, cx))
            });
            vcx.run_until_parked();

            let (content, height, chrome) = vcx.update(|_window, cx| {
                let editor = editor.read(cx);
                let rows = editor.field_rows.clone().unwrap_or(0..=0);
                let pad_y = editor.pad_y();
                (
                    editor.content().to_owned(),
                    editor.field_height(&rows),
                    pad_y + pad_y + BORDER_THIN + BORDER_THIN,
                )
            });
            assert!(content == pasted, "paste of {} chars altered", pasted.len());
            assert_eq!(height, LINE * shown + chrome, "{} chars", pasted.len());
        }
    }

    #[gpui::test]
    fn insert_at_caret_splices_the_text_at_the_caret_and_leaves_the_caret_after_it(
        cx: &mut TestAppContext,
    ) {
        const TOKEN: &str = "%user_id%";
        install_presentation(cx);
        let (editor, vcx) = cx
            .add_window_view(|_window, cx| CodeEditor::new(Language::Rhai, "", cx).compact_field());

        let after = vcx.update(|_window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_content("a + b", cx);
                editor.buffer.move_to(4);
                editor.insert_at_caret(TOKEN, cx);
                (editor.content().to_owned(), editor.buffer.cursor())
            })
        });

        assert_eq!(after, (format!("a + {TOKEN}b"), 4 + TOKEN.len()));
    }
}
