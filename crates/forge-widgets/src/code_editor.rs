use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding,
    widget::{column, container, text, text_editor},
};

use crate::{
    ForgePalette,
    tokens::{FONT_BODY, FONT_CAPS_SM, FontRole, font},
};

pub struct CodeEditorState {
    pub content: text_editor::Content,
}

impl CodeEditorState {
    pub fn new() -> Self {
        Self {
            content: text_editor::Content::new(),
        }
    }

    pub fn with_text(initial: &str) -> Self {
        Self {
            content: text_editor::Content::with_text(initial),
        }
    }

    pub fn text(&self) -> String {
        self.content.text()
    }

    pub fn line_count(&self) -> usize {
        self.content.line_count()
    }

    pub fn cursor_position(&self) -> (usize, usize) {
        let c = self.content.cursor();
        (c.position.line, c.position.column)
    }
}

impl Default for CodeEditorState {
    fn default() -> Self {
        Self::new()
    }
}

/// Line-number gutter + styled `text_editor` side-by-side.
///
/// The gutter renders all line numbers as a non-scrollable column; it does not
/// scroll-sync with the editor's internal scroll in alpha-6. For short scripts
/// (< ~60 lines) this is invisible; beta adds a custom Highlighter that owns
/// the gutter inside the editor pipeline.
pub fn code_editor<'a, Msg: Clone + 'a>(
    palette: &'a ForgePalette,
    state: &'a CodeEditorState,
    on_action: impl Fn(text_editor::Action) -> Msg + 'a,
) -> Element<'a, Msg> {
    let mono = font(FontRole::Monospace);
    let line_count = state.line_count();

    let number_fg = palette.text_extreme_faint;
    let base = palette.base;
    let text_primary = palette.text_primary;
    let brand = palette.brand;

    let gutter_items: Vec<Element<'a, Msg>> = (1..=line_count)
        .map(|n| {
            container(
                text(n.to_string())
                    .font(mono)
                    .size(FONT_CAPS_SM)
                    .color(number_fg),
            )
            .width(38.0_f32)
            .padding(Padding {
                top: 0.0,
                bottom: 0.0,
                left: 0.0,
                right: 14.0,
            })
            .align_x(Alignment::End)
            .into()
        })
        .collect();

    let gutter = container(column(gutter_items))
        .padding(Padding {
            top: 10.0,
            bottom: 10.0,
            left: 0.0,
            right: 0.0,
        })
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(base)),
            border: Border::default(),
            ..container::Style::default()
        });

    let editor = text_editor(&state.content)
        .on_action(on_action)
        .font(mono)
        .size(FONT_BODY)
        .height(Length::Fill)
        .style(move |_: &iced::Theme, _status| text_editor::Style {
            background: Background::Color(base),
            border: Border::default(),
            placeholder: Color::TRANSPARENT,
            value: text_primary,
            selection: Color { a: 0.2, ..brand },
        });

    iced::widget::row![gutter, editor].into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn new_state_has_one_line() {
        let s = CodeEditorState::new();
        assert_eq!(s.line_count(), 1);
    }

    #[test]
    fn with_text_two_lines_line_count() {
        let s = CodeEditorState::with_text("hello\nworld");
        assert_eq!(s.line_count(), 2);
    }

    #[test]
    fn with_text_text_contains_content() {
        let s = CodeEditorState::with_text("hello\nworld");
        assert!(s.text().starts_with("hello\nworld"));
    }

    #[test]
    fn default_equals_new() {
        let d = CodeEditorState::default();
        assert_eq!(d.line_count(), 1);
    }

    #[test]
    fn cursor_position_new_is_origin() {
        let s = CodeEditorState::new();
        let (line, col) = s.cursor_position();
        assert_eq!(line, 0);
        assert_eq!(col, 0);
    }

    #[test]
    fn code_editor_widget_compiles_empty() {
        let state = CodeEditorState::new();
        let _: Element<'_, text_editor::Action> = code_editor(&CATPPUCCIN_MOCHA, &state, |a| a);
    }

    #[test]
    fn code_editor_widget_compiles_with_content() {
        let state = CodeEditorState::with_text("fn main() {\n    let x = 42;\n}");
        let _: Element<'_, text_editor::Action> = code_editor(&CATPPUCCIN_MOCHA, &state, |a| a);
    }
}
