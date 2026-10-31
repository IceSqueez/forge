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
    use crate::palette::{AccentColor, FORGE_DEFAULT, ThemeId};

    #[test]
    fn code_classes_paint_the_design_colors_of_the_default_theme_in_every_code_language() {
        let expected = [
            (TokenClass::Keyword, 0x00c9_a6f0),
            (TokenClass::Function, 0x008b_e9fd),
            (TokenClass::String, 0x0050_fa7b),
            (TokenClass::Number, 0x00ff_66d9),
            (TokenClass::Comment, 0x006b_6884),
            (TokenClass::Variable, 0x00ff_b86c),
            (TokenClass::Plain, 0x00f0_eef8),
            (TokenClass::Punctuation, 0x00c9_c4dd),
        ];
        for language in [
            Language::Html,
            Language::Css,
            Language::JavaScript,
            Language::Rhai,
        ] {
            for (class, color) in expected {
                assert_eq!(
                    token_color(language, class, &FORGE_DEFAULT),
                    Hsla::from(gpui::rgb(color)),
                    "{language:?} {class:?}"
                );
            }
        }
    }

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
}
