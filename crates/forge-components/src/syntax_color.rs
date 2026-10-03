use std::ops::Range;

use gpui::{HighlightStyle, Hsla, SharedString, StyledText};

use crate::highlight::{Language, TokenClass, highlight};
use crate::palette::ForgePalette;

pub fn token_color(language: Language, class: TokenClass, palette: &ForgePalette) -> Hsla {
    let color = match (language, class) {
        (Language::Json, TokenClass::Variable) => palette.info,
        (Language::Json, TokenClass::String) => palette.success,
        (Language::Json, TokenClass::Number) => palette.bits,
        (Language::Json, TokenClass::Keyword) => palette.brand,
        (_, TokenClass::Keyword) => palette.code_keyword,
        (_, TokenClass::Function) => palette.code_fn,
        (_, TokenClass::String) => palette.code_str,
        (_, TokenClass::Number) => palette.code_num,
        (_, TokenClass::Comment) => palette.code_comment,
        (_, TokenClass::Variable) => palette.code_var,
        (_, TokenClass::Plain) => palette.text_primary,
        (_, TokenClass::Punctuation) => palette.text_secondary,
    };
    color.into()
}

pub fn syntax_runs(language: Language, text: &str, palette: &ForgePalette) -> Vec<(usize, Hsla)> {
    highlight(language, text)
        .into_iter()
        .map(|span| (span.len, token_color(language, span.class, palette)))
        .collect()
}

pub fn highlighted_text(
    language: Language,
    text: impl Into<SharedString>,
    palette: &ForgePalette,
) -> StyledText {
    let text = text.into();
    let mut offset = 0usize;
    let highlights: Vec<(Range<usize>, HighlightStyle)> = syntax_runs(language, &text, palette)
        .into_iter()
        .map(|(len, color)| {
            let start = offset;
            offset += len;
            (start..offset, HighlightStyle::from(color))
        })
        .collect();
    StyledText::new(text).with_highlights(highlights)
}
