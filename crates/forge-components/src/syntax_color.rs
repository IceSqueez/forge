use std::ops::Range;

use gpui::{HighlightStyle, Hsla, SharedString, StyledText};

use crate::highlight::{Language, TokenClass, highlight};
use crate::palette::ForgePalette;

pub(crate) fn token_color(language: Language, class: TokenClass, palette: &ForgePalette) -> Hsla {
    let color = match (language, class) {
        (Language::Json, TokenClass::Variable) => palette.info,
        (Language::Json, TokenClass::String) => palette.success,
        (Language::Json, TokenClass::Number) => palette.accent_peach,
        (Language::Json, TokenClass::Keyword) => palette.theme_brand,
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

pub(crate) fn syntax_runs(
    language: Language,
    text: &str,
    palette: &ForgePalette,
) -> Vec<(usize, Hsla)> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::{AccentColor, ThemeId};

    const CLASSES: [TokenClass; 8] = [
        TokenClass::Keyword,
        TokenClass::Function,
        TokenClass::String,
        TokenClass::Number,
        TokenClass::Comment,
        TokenClass::Variable,
        TokenClass::Plain,
        TokenClass::Punctuation,
    ];

    #[test]
    fn json_classes_use_data_viewer_colors_in_every_theme_whatever_the_accent() {
        for theme in ThemeId::ALL {
            let base = theme.palette();
            for accent in AccentColor::ALL {
                let palette = theme.accented_palette(accent);
                for (class, expected) in [
                    (TokenClass::Variable, base.info),
                    (TokenClass::String, base.success),
                    (TokenClass::Number, base.accent_peach),
                    (TokenClass::Keyword, base.theme_brand),
                    (TokenClass::Plain, base.text_primary),
                    (TokenClass::Punctuation, base.text_secondary),
                ] {
                    assert_eq!(
                        token_color(Language::Json, class, &palette),
                        Hsla::from(expected),
                        "{theme:?} {accent:?} {class:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn code_classes_use_code_tokens_in_every_theme() {
        for theme in ThemeId::ALL {
            let palette = theme.palette();
            for language in [
                Language::Html,
                Language::Css,
                Language::JavaScript,
                Language::Rhai,
            ] {
                for class in CLASSES {
                    let expected = match class {
                        TokenClass::Keyword => palette.code_keyword,
                        TokenClass::Function => palette.code_fn,
                        TokenClass::String => palette.code_str,
                        TokenClass::Number => palette.code_num,
                        TokenClass::Comment => palette.code_comment,
                        TokenClass::Variable => palette.code_var,
                        TokenClass::Plain => palette.text_primary,
                        TokenClass::Punctuation => palette.text_secondary,
                    };
                    assert_eq!(
                        token_color(language, class, &palette),
                        Hsla::from(expected),
                        "{theme:?} {language:?} {class:?}"
                    );
                }
            }
        }
    }
}
