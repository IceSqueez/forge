use std::ops::Range;
use std::sync::Arc;

use forge_script::{
    MethodDescriptor, SymbolKind, SymbolToken, UserFunctionSig, catalog, resolve_symbol_from_tokens,
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
use crate::hover_popover::{HoverTarget, hover_popover};
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
    pub autocomplete_visible: bool,
    pub user_functions: Vec<UserFunctionSig>,
}

impl ScriptEditorWidgetState {
    pub fn new() -> Self {
        Self {
            editor: CodeEditorState::new(),
            annotation_diagnostics: Vec::new(),
            error_lines: Vec::new(),
            autocomplete: AutocompletePopupState::default(),
            overlay_dismissed: false,
            autocomplete_visible: false,
            user_functions: Vec::new(),
        }
    }

    pub fn with_text(initial: &str) -> Self {
        Self {
            editor: CodeEditorState::with_text(initial),
            annotation_diagnostics: Vec::new(),
            error_lines: Vec::new(),
            autocomplete: AutocompletePopupState::default(),
            overlay_dismissed: false,
            autocomplete_visible: false,
            user_functions: Vec::new(),
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
    CtrlSpacePressed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayChoice {
    None,
    Hover,
    Autocomplete,
}

/// Hover takes priority when cursor is on a resolved symbol (catalog or user-defined);
/// autocomplete is shown otherwise when candidates exist and `prefix_len >= 1`.
pub fn choose_overlay(
    has_hover: bool,
    candidates: &[&'static MethodDescriptor],
    prefix_len: usize,
) -> OverlayChoice {
    if has_hover {
        OverlayChoice::Hover
    } else if !candidates.is_empty() && prefix_len >= 1 {
        OverlayChoice::Autocomplete
    } else {
        OverlayChoice::None
    }
}

fn resolve_user_fn_hover<'a>(
    rhai_tokens: &[(Range<usize>, RhaiTokenKind)],
    line_text: &str,
    col: usize,
    user_functions: &'a [UserFunctionSig],
) -> Option<&'a UserFunctionSig> {
    let (tok_range, tok_kind) = rhai_tokens.iter().find(|(r, _)| r.contains(&col))?;
    if *tok_kind != RhaiTokenKind::FunctionCall {
        return None;
    }
    let fn_name = &line_text[tok_range.clone()];
    user_functions.iter().find(|f| f.name == fn_name)
}

/// Dots after `)` are not triggered — `foo().` does not open completion by default.
pub fn should_trigger_autocomplete(
    line_text: &str,
    cursor_col: usize,
    just_typed: Option<char>,
    ctrl_space_pressed: bool,
) -> bool {
    if ctrl_space_pressed {
        return true;
    }
    let Some(ch) = just_typed else {
        return false;
    };
    let bytes = line_text.as_bytes();
    let pos = cursor_col.min(bytes.len());
    match ch {
        '.' => {
            if pos < 2 {
                return false;
            }
            let prev = bytes[pos - 2];
            prev.is_ascii_alphanumeric() || prev == b'_'
        }
        ':' => {
            if pos < 3 {
                return false;
            }
            let prev = bytes[pos - 2];
            let prev_prev = bytes[pos - 3];
            prev == b':' && (prev_prev.is_ascii_alphanumeric() || prev_prev == b'_')
        }
        _ => false,
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
    let catalog_hover = resolve_symbol_from_tokens(&sym_tokens, &line_text, col);
    let user_hover = if catalog_hover.is_none() {
        resolve_user_fn_hover(&rhai_tokens, &line_text, col, &state.user_functions)
    } else {
        None
    };
    let has_hover = catalog_hover.is_some() || user_hover.is_some();

    let prefix = prefix_under_cursor(&line_text, col);
    let candidates: Vec<&'static MethodDescriptor> =
        if state.autocomplete_visible && !state.overlay_dismissed && !prefix.is_empty() {
            filter_candidates(catalog(), &prefix)
        } else {
            Vec::new()
        };

    let choice = if state.overlay_dismissed {
        OverlayChoice::None
    } else {
        choose_overlay(has_hover, &candidates, prefix.len())
    };

    let inner: Element<'a, Msg> =
        crate::code_editor::rhai_editor(palette, &state.editor, &state.error_lines, move |a| {
            on_message(ScriptEditorWidgetMsg::EditorAction(a))
        });

    let (overlay_panel, is_autocomplete): (Option<Element<'a, Msg>>, bool) = match choice {
        OverlayChoice::Hover => {
            let target = catalog_hover
                .map(HoverTarget::Catalog)
                .or_else(|| user_hover.map(HoverTarget::User));
            (target.map(|t| hover_popover(t, palette)), false)
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
        use iced::keyboard::key::Named;

        if let Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(Named::Space),
            modifiers,
            ..
        }) = event
            && modifiers.control()
        {
            shell.publish((self.on_message)(ScriptEditorWidgetMsg::CtrlSpacePressed));
            shell.capture_event();
            return;
        }

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
    fn trigger_on_dot_after_identifier() {
        assert!(should_trigger_autocomplete("arr.", 4, Some('.'), false));
    }

    #[test]
    fn trigger_on_dot_after_whitespace() {
        assert!(!should_trigger_autocomplete("   .", 4, Some('.'), false));
    }

    #[test]
    fn trigger_on_dot_after_paren() {
        assert!(!should_trigger_autocomplete("foo().", 6, Some('.'), false));
    }

    #[test]
    fn trigger_on_double_colon_after_identifier() {
        assert!(should_trigger_autocomplete(
            "globals::",
            9,
            Some(':'),
            false
        ));
    }

    #[test]
    fn trigger_on_single_colon_no_trigger() {
        assert!(!should_trigger_autocomplete(
            "let q ::",
            9,
            Some(':'),
            false
        ));
    }

    #[test]
    fn trigger_ctrl_space_forces_open() {
        assert!(should_trigger_autocomplete("let x = 1", 9, None, true));
        assert!(should_trigger_autocomplete("", 0, None, true));
    }

    #[test]
    fn trigger_after_non_trigger_char_no_trigger() {
        assert!(!should_trigger_autocomplete(
            "let q = 12",
            10,
            Some('2'),
            false
        ));
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
        static D2: MethodDescriptor = MethodDescriptor {
            namespace: Some("globals"),
            name: "set",
            kind: SymbolKind::Fn,
            params: EMPTY_PARAMS,
            return_type: "()",
            doc: None,
        };
        let candidates = vec![&D2];
        let result = choose_overlay(true, &candidates, 3);
        assert_eq!(result, OverlayChoice::Hover);
    }

    #[test]
    fn choose_overlay_none_when_no_hover_no_candidates() {
        let result = choose_overlay(false, &[], 5);
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
        let result = choose_overlay(false, &candidates, 2);
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
        let result = choose_overlay(false, &candidates, 0);
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

    #[test]
    fn resolve_user_function_call_finds_matching_fn() {
        use forge_script::UserFunctionSig;

        let line = "double(x)";
        let tokens = crate::rhai_highlight::tokenize_line(line, false).0;
        let user_fns = vec![UserFunctionSig {
            name: "double".to_owned(),
            params: vec![],
            return_type: Some("int".to_owned()),
            doc: None,
        }];
        let result = resolve_user_fn_hover(&tokens, line, 3, &user_fns);
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "double");
    }

    #[test]
    fn autocomplete_pipeline_500_line_within_budget() {
        use std::time::Instant;

        let lines = build_pipeline_test_lines();
        let catalog_entries = catalog();

        let t0 = Instant::now();
        for i in 0..100usize {
            let line = &lines[i % lines.len()];
            let col = (line.len() / 2).max(1);

            let (raw_tokens, _) = tokenize_line(line, false);
            let sym_tokens = to_symbol_tokens(&raw_tokens);
            let last_char = line.chars().last();
            let _ = should_trigger_autocomplete(line, col, last_char, false);
            let prefix = prefix_under_cursor(line, col);
            let _candidates = filter_candidates(catalog_entries, &prefix);
            let _hover = resolve_symbol_from_tokens(&sym_tokens, line, col);
        }
        let total_ms = t0.elapsed().as_millis();
        assert!(
            total_ms <= 500,
            "autocomplete pipeline 100 keystrokes took {total_ms}ms, budget is 500ms (5ms avg)"
        );
    }

    fn build_pipeline_test_lines() -> Vec<String> {
        let mut lines = Vec::with_capacity(500);
        for i in 0..500usize {
            let line = match i % 10 {
                0 => format!("let x_{i} = forge::globals::get(\"key_{i}\");"),
                1 => format!("forge::globals::set(\"key_{i}\", {i});"),
                2 => format!("forge::chat::send(\"hello {i}\");"),
                3 => format!("forge::tts::speak(\"msg {i}\");"),
                4 => format!("// single-line comment {i}"),
                5 => format!("let f_{i} = {};", (i as f64) * 1.5),
                6 => format!("let b_{i} = {};", if i % 2 == 0 { "true" } else { "false" }),
                7 => format!("forge::log(\"step {i}\");"),
                8 => format!("forge::globals::incr(\"counter_{i}\", 1);"),
                _ => format!("let s_{i} = \"value_{i}\";"),
            };
            lines.push(line);
        }
        lines
    }
}
