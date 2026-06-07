use std::ops::Range;
use std::sync::Arc;

use forge_script::{
    MethodDescriptor, SymbolKind, SymbolToken, catalog, resolve_symbol_from_tokens,
};
use forge_types::AnnotationDiagnostic;
use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::Widget;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Layout, Shell};
use iced::widget::text_editor;
use iced::{Element, Event, Length, Rectangle, Size, Vector};

use crate::autocomplete_popup::{AutocompletePopupState, autocomplete_popup, filter_candidates};
use crate::code_editor::CodeEditorState;
use crate::hover_popover::hover_popover;
use crate::palette::ForgePalette;
use crate::rhai_highlight::{RhaiTokenKind, tokenize_line};
use crate::script_editor_overlay::clamp_to_bounds;
use crate::tokens::{FONT_SM, Spacing, spf};

const GUTTER_WIDTH: f32 = 38.0;
const LINE_HEIGHT: f32 = FONT_SM * 1.4;
const CHAR_WIDTH: f32 = FONT_SM * 0.6;

pub struct ScriptEditorWidgetState {
    pub editor: CodeEditorState,
    pub annotation_diagnostics: Vec<AnnotationDiagnostic>,
    pub error_lines: Vec<usize>,
    pub autocomplete: AutocompletePopupState,
    pub overlay_dismissed: bool,
}

impl ScriptEditorWidgetState {
    pub fn new() -> Self {
        Self {
            editor: CodeEditorState::new(),
            annotation_diagnostics: Vec::new(),
            error_lines: Vec::new(),
            autocomplete: AutocompletePopupState::default(),
            overlay_dismissed: false,
        }
    }

    pub fn with_text(initial: &str) -> Self {
        Self {
            editor: CodeEditorState::with_text(initial),
            annotation_diagnostics: Vec::new(),
            error_lines: Vec::new(),
            autocomplete: AutocompletePopupState::default(),
            overlay_dismissed: false,
        }
    }
}

impl Default for ScriptEditorWidgetState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum ScriptEditorWidgetMsg {
    EditorAction(text_editor::Action),
    AutocompleteSelectionUp,
    AutocompleteSelectionDown,
    AutocompleteInsert(MethodDescriptor),
    OverlayDismissed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayChoice {
    None,
    Hover,
    Autocomplete,
}

/// Hover takes priority when cursor is on a resolved symbol; autocomplete is
/// shown otherwise when candidates exist and `prefix_len >= 1`.
pub fn choose_overlay(
    hover: Option<&'static MethodDescriptor>,
    candidates: &[&'static MethodDescriptor],
    prefix_len: usize,
) -> OverlayChoice {
    if hover.is_some() {
        OverlayChoice::Hover
    } else if !candidates.is_empty() && prefix_len >= 1 {
        OverlayChoice::Autocomplete
    } else {
        OverlayChoice::None
    }
}

pub fn prefix_under_cursor(line: &str, col: usize) -> String {
    let bytes = line.as_bytes();
    let end = col.min(bytes.len());
    let mut start = end;
    while start > 0 {
        let b = bytes[start - 1];
        if b.is_ascii_alphanumeric() || b == b'_' || b == b':' {
            start -= 1;
        } else {
            break;
        }
    }
    line[start..end].to_owned()
}

pub fn apply_autocomplete_insert(
    prefix: &str,
    descriptor: &MethodDescriptor,
) -> Vec<text_editor::Action> {
    use iced::widget::text_editor::{Edit, Motion};

    let prefix_chars = prefix.chars().count();

    let base = match descriptor.namespace {
        Some(ns) => format!("{ns}::{}", descriptor.name),
        None => descriptor.name.to_string(),
    };

    let insert_text = match descriptor.kind {
        SymbolKind::Fn => format!("{base}()"),
        SymbolKind::Property => base,
    };

    let mut actions: Vec<text_editor::Action> = Vec::with_capacity(prefix_chars + 2);
    for _ in 0..prefix_chars {
        actions.push(text_editor::Action::Select(Motion::Left));
    }
    actions.push(text_editor::Action::Edit(Edit::Paste(Arc::new(
        insert_text,
    ))));
    if matches!(descriptor.kind, SymbolKind::Fn) {
        actions.push(text_editor::Action::Move(Motion::Left));
    }
    actions
}

fn rhai_kind_to_symbol(kind: RhaiTokenKind) -> SymbolToken {
    match kind {
        RhaiTokenKind::Namespace => SymbolToken::Namespace,
        RhaiTokenKind::FunctionCall => SymbolToken::FunctionCall,
        RhaiTokenKind::Identifier => SymbolToken::Identifier,
        _ => SymbolToken::Other,
    }
}

fn to_symbol_tokens(tokens: &[(Range<usize>, RhaiTokenKind)]) -> Vec<(Range<usize>, SymbolToken)> {
    tokens
        .iter()
        .map(|(r, k)| (r.clone(), rhai_kind_to_symbol(*k)))
        .collect()
}

pub fn script_editor_widget<'a, Msg: Clone + 'a>(
    state: &'a ScriptEditorWidgetState,
    palette: &'a ForgePalette,
    on_message: impl Fn(ScriptEditorWidgetMsg) -> Msg + 'static + Copy,
) -> Element<'a, Msg> {
    let (line, col) = state.editor.cursor_position();
    let line_text = state.editor.line_text(line).unwrap_or_default();

    let rhai_tokens = tokenize_line(&line_text, false).0;
    let sym_tokens = to_symbol_tokens(&rhai_tokens);
    let hover = resolve_symbol_from_tokens(&sym_tokens, &line_text, col);

    let prefix = prefix_under_cursor(&line_text, col);
    let candidates: Vec<&'static MethodDescriptor> =
        if !state.overlay_dismissed && !prefix.is_empty() {
            filter_candidates(catalog(), &prefix)
        } else {
            Vec::new()
        };

    let choice = if state.overlay_dismissed {
        OverlayChoice::None
    } else {
        choose_overlay(hover, &candidates, prefix.len())
    };

    let inner: Element<'a, Msg> =
        crate::code_editor::rhai_editor(palette, &state.editor, &state.error_lines, move |a| {
            on_message(ScriptEditorWidgetMsg::EditorAction(a))
        });

    let (overlay_panel, is_autocomplete): (Option<Element<'a, Msg>>, bool) = match choice {
        OverlayChoice::Hover => {
            if let Some(desc) = hover {
                (Some(hover_popover(desc, palette)), false)
            } else {
                (None, false)
            }
        }
        OverlayChoice::Autocomplete => {
            let panel = autocomplete_popup(
                &state.autocomplete,
                &candidates,
                move |msg| {
                    use crate::autocomplete_popup::AutocompletePopupMessage;
                    match msg {
                        AutocompletePopupMessage::Insert(d) => {
                            on_message(ScriptEditorWidgetMsg::AutocompleteInsert(d))
                        }
                        AutocompletePopupMessage::SelectionUp => {
                            on_message(ScriptEditorWidgetMsg::AutocompleteSelectionUp)
                        }
                        AutocompletePopupMessage::SelectionDown => {
                            on_message(ScriptEditorWidgetMsg::AutocompleteSelectionDown)
                        }
                        AutocompletePopupMessage::FilterChanged(_) => {
                            on_message(ScriptEditorWidgetMsg::OverlayDismissed)
                        }
                    }
                },
                palette,
            );
            (Some(panel), true)
        }
        OverlayChoice::None => (None, false),
    };

    ScriptEditorWidgetInner {
        inner,
        overlay_panel,
        candidates,
        state,
        anchor_line: line,
        anchor_col: col,
        is_autocomplete,
        on_message: Box::new(on_message),
    }
    .into()
}

struct ScriptEditorWidgetInner<'a, Msg: Clone> {
    inner: Element<'a, Msg>,
    overlay_panel: Option<Element<'a, Msg>>,
    candidates: Vec<&'static MethodDescriptor>,
    state: &'a ScriptEditorWidgetState,
    anchor_line: usize,
    anchor_col: usize,
    is_autocomplete: bool,
    on_message: Box<dyn Fn(ScriptEditorWidgetMsg) -> Msg>,
}

impl<'a, Msg: Clone + 'a> From<ScriptEditorWidgetInner<'a, Msg>> for Element<'a, Msg> {
    fn from(w: ScriptEditorWidgetInner<'a, Msg>) -> Self {
        Element::new(w)
    }
}

impl<'a, Msg: Clone + 'a> Widget<Msg, iced::Theme, iced::Renderer>
    for ScriptEditorWidgetInner<'a, Msg>
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::stateless()
    }

    fn state(&self) -> tree::State {
        tree::State::None
    }

    fn children(&self) -> Vec<Tree> {
        let mut v = vec![Tree::new(&self.inner)];
        if let Some(p) = &self.overlay_panel {
            v.push(Tree::new(p));
        }
        v
    }

    fn diff(&self, tree: &mut Tree) {
        match &self.overlay_panel {
            None => tree.diff_children(std::slice::from_ref(&self.inner)),
            Some(p) => tree.diff_children(&[&self.inner, p]),
        }
    }

    fn size(&self) -> iced::Size<Length> {
        self.inner.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.inner
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.inner.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg>,
        viewport: &Rectangle,
    ) {
        self.inner.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.inner.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        _renderer: &iced::Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Msg, iced::Theme, iced::Renderer>> {
        let panel = self.overlay_panel.as_mut()?;
        let panel_tree = tree.children.get_mut(1)?;
        let editor_bounds = layout.bounds() + translation;
        Some(overlay::Element::new(Box::new(ScriptEditorInlineOverlay {
            panel,
            panel_tree,
            editor_bounds,
            anchor_line: self.anchor_line,
            anchor_col: self.anchor_col,
            candidates: &self.candidates,
            selected_idx: self.state.autocomplete.selected_idx,
            is_autocomplete: self.is_autocomplete,
            on_message: self.on_message.as_ref(),
        })))
    }
}

struct ScriptEditorInlineOverlay<'b, 'a: 'b, Msg: Clone> {
    panel: &'b mut Element<'a, Msg>,
    panel_tree: &'b mut Tree,
    editor_bounds: Rectangle,
    anchor_line: usize,
    anchor_col: usize,
    candidates: &'b [&'static MethodDescriptor],
    selected_idx: usize,
    is_autocomplete: bool,
    on_message: &'b dyn Fn(ScriptEditorWidgetMsg) -> Msg,
}

impl<Msg: Clone> overlay::Overlay<Msg, iced::Theme, iced::Renderer>
    for ScriptEditorInlineOverlay<'_, '_, Msg>
{
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let limits = layout::Limits::new(Size::ZERO, Size::INFINITE);
        let content_node = self
            .panel
            .as_widget_mut()
            .layout(self.panel_tree, renderer, &limits);
        let panel_sz = content_node.size();

        let top_padding = spf(Spacing::Sm);
        let anchor_x = self.editor_bounds.x + GUTTER_WIDTH + self.anchor_col as f32 * CHAR_WIDTH;
        let anchor_y = self.editor_bounds.y + top_padding + self.anchor_line as f32 * LINE_HEIGHT;

        let position = clamp_to_bounds(
            Rectangle {
                x: anchor_x,
                y: anchor_y + LINE_HEIGHT,
                width: panel_sz.width,
                height: panel_sz.height,
            },
            bounds,
            anchor_y,
        );
        content_node.move_to(position)
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        self.panel.as_widget().draw(
            self.panel_tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &bounds,
        );
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg>,
    ) {
        use iced::keyboard::key::Named;

        if let Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event {
            match key {
                iced::keyboard::Key::Named(Named::ArrowUp) if self.is_autocomplete => {
                    shell.publish((self.on_message)(
                        ScriptEditorWidgetMsg::AutocompleteSelectionUp,
                    ));
                    shell.capture_event();
                    return;
                }
                iced::keyboard::Key::Named(Named::ArrowDown) if self.is_autocomplete => {
                    shell.publish((self.on_message)(
                        ScriptEditorWidgetMsg::AutocompleteSelectionDown,
                    ));
                    shell.capture_event();
                    return;
                }
                iced::keyboard::Key::Named(Named::Tab)
                | iced::keyboard::Key::Named(Named::Enter)
                    if self.is_autocomplete =>
                {
                    if let Some(d) = self.candidates.get(self.selected_idx) {
                        shell.publish((self.on_message)(
                            ScriptEditorWidgetMsg::AutocompleteInsert(**d),
                        ));
                        shell.capture_event();
                    }
                    return;
                }
                iced::keyboard::Key::Named(Named::Escape) => {
                    shell.publish((self.on_message)(ScriptEditorWidgetMsg::OverlayDismissed));
                    shell.capture_event();
                    return;
                }
                _ => {}
            }
        }

        let content_bounds = layout.bounds();

        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && let mouse::Cursor::Available(pos) = cursor
            && !content_bounds.contains(pos)
        {
            shell.publish((self.on_message)(ScriptEditorWidgetMsg::OverlayDismissed));
            shell.capture_event();
            return;
        }

        self.panel.as_widget_mut().update(
            self.panel_tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &content_bounds,
        );
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        self.panel
            .as_widget()
            .mouse_interaction(self.panel_tree, layout, cursor, &bounds, renderer)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_script::{ParamDescriptor, SymbolKind};

    const EMPTY_PARAMS: &[ParamDescriptor] = &[];

    fn fn_desc(namespace: Option<&'static str>, name: &'static str) -> MethodDescriptor {
        MethodDescriptor {
            namespace,
            name,
            kind: SymbolKind::Fn,
            params: EMPTY_PARAMS,
            return_type: "()",
            doc: None,
        }
    }

    fn prop_desc(name: &'static str) -> MethodDescriptor {
        MethodDescriptor {
            namespace: None,
            name,
            kind: SymbolKind::Property,
            params: EMPTY_PARAMS,
            return_type: "Int",
            doc: None,
        }
    }

    #[test]
    fn prefix_under_cursor_simple() {
        assert_eq!(
            prefix_under_cursor("globals::get_val", 16),
            "globals::get_val"
        );
    }

    #[test]
    fn prefix_under_cursor_no_identifier() {
        assert_eq!(prefix_under_cursor("let x = ", 8), "");
    }

    #[test]
    fn prefix_under_cursor_mid_token() {
        assert_eq!(prefix_under_cursor("globals::get_val", 11), "globals::ge");
    }

    #[test]
    fn prefix_under_cursor_col_zero() {
        assert_eq!(prefix_under_cursor("globals", 0), "");
    }

    #[test]
    fn prefix_under_cursor_col_beyond_line() {
        assert_eq!(prefix_under_cursor("abc", 100), "abc");
    }

    #[test]
    fn autocomplete_insert_function_adds_parens() {
        let desc = fn_desc(Some("globals"), "get");
        let actions = apply_autocomplete_insert("glob", &desc);
        let has_paste = actions.iter().any(|a| {
            if let text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(s)) = a {
                s.contains("globals::get()")
            } else {
                false
            }
        });
        assert!(has_paste, "paste action must include 'globals::get()'");
        let cursor_inside = actions
            .last()
            .map(|a| matches!(a, text_editor::Action::Move(_)))
            .unwrap_or(false);
        assert!(cursor_inside, "last action must move cursor inside parens");
    }

    #[test]
    fn autocomplete_insert_property_no_parens() {
        let desc = prop_desc("len");
        let actions = apply_autocomplete_insert("le", &desc);
        let has_paste = actions.iter().any(|a| {
            if let text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(s)) = a {
                *s.as_ref() == "len"
            } else {
                false
            }
        });
        assert!(has_paste, "paste action must be 'len'");
        let no_move = actions
            .last()
            .map(|a| !matches!(a, text_editor::Action::Move(_)))
            .unwrap_or(true);
        assert!(
            no_move,
            "property insert must not move cursor after insertion"
        );
    }

    #[test]
    fn symbol_resolution_priority_over_autocomplete() {
        static D1: MethodDescriptor = MethodDescriptor {
            namespace: Some("globals"),
            name: "get",
            kind: SymbolKind::Fn,
            params: EMPTY_PARAMS,
            return_type: "Variant",
            doc: None,
        };
        static D2: MethodDescriptor = MethodDescriptor {
            namespace: Some("globals"),
            name: "set",
            kind: SymbolKind::Fn,
            params: EMPTY_PARAMS,
            return_type: "()",
            doc: None,
        };
        let candidates = vec![&D2];
        let result = choose_overlay(Some(&D1), &candidates, 3);
        assert_eq!(result, OverlayChoice::Hover);
    }

    #[test]
    fn choose_overlay_none_when_no_hover_no_candidates() {
        let result = choose_overlay(None, &[], 5);
        assert_eq!(result, OverlayChoice::None);
    }

    #[test]
    fn choose_overlay_autocomplete_when_candidates_and_prefix() {
        static D: MethodDescriptor = MethodDescriptor {
            namespace: None,
            name: "log",
            kind: SymbolKind::Fn,
            params: EMPTY_PARAMS,
            return_type: "()",
            doc: None,
        };
        let candidates = vec![&D];
        let result = choose_overlay(None, &candidates, 2);
        assert_eq!(result, OverlayChoice::Autocomplete);
    }

    #[test]
    fn choose_overlay_none_when_zero_prefix_len() {
        static D: MethodDescriptor = MethodDescriptor {
            namespace: None,
            name: "log",
            kind: SymbolKind::Fn,
            params: EMPTY_PARAMS,
            return_type: "()",
            doc: None,
        };
        let candidates = vec![&D];
        let result = choose_overlay(None, &candidates, 0);
        assert_eq!(result, OverlayChoice::None);
    }

    #[test]
    fn prefix_insert_select_count_matches_prefix_chars() {
        let desc = fn_desc(Some("chat"), "send");
        let prefix = "ch";
        let actions = apply_autocomplete_insert(prefix, &desc);
        let select_count = actions
            .iter()
            .filter(|a| matches!(a, text_editor::Action::Select(_)))
            .count();
        assert_eq!(select_count, prefix.chars().count());
    }
}
