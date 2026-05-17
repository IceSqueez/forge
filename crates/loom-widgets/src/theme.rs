use iced::Theme;
use iced::theme::Palette;

use crate::palette::{CATPPUCCIN_MOCHA, LATTE, LoomPalette, TOKYO_NIGHT};
use crate::tokens::ThemeId;

pub fn catppuccin_mocha() -> (Theme, LoomPalette) {
    let palette = Palette {
        background: CATPPUCCIN_MOCHA.base,
        text: CATPPUCCIN_MOCHA.text_primary,
        primary: CATPPUCCIN_MOCHA.brand,
        success: CATPPUCCIN_MOCHA.success,
        warning: CATPPUCCIN_MOCHA.warning,
        danger: CATPPUCCIN_MOCHA.random,
    };
    (
        Theme::custom("Catppuccin Mocha".to_owned(), palette),
        CATPPUCCIN_MOCHA,
    )
}

pub fn tokyo_night_storm() -> (Theme, LoomPalette) {
    let palette = Palette {
        background: TOKYO_NIGHT.base,
        text: TOKYO_NIGHT.text_primary,
        primary: TOKYO_NIGHT.brand,
        success: TOKYO_NIGHT.success,
        warning: TOKYO_NIGHT.warning,
        danger: TOKYO_NIGHT.random,
    };
    (
        Theme::custom("Tokyo Night Storm".to_owned(), palette),
        TOKYO_NIGHT,
    )
}

pub fn latte() -> (Theme, LoomPalette) {
    let palette = Palette {
        background: LATTE.base,
        text: LATTE.text_primary,
        primary: LATTE.brand,
        success: LATTE.success,
        warning: LATTE.warning,
        danger: LATTE.random,
    };
    (Theme::custom("Latte".to_owned(), palette), LATTE)
}

pub fn palette_for_theme(theme_id: ThemeId) -> &'static LoomPalette {
    match theme_id {
        ThemeId::CatppuccinMocha => &CATPPUCCIN_MOCHA,
        ThemeId::TokyoNight => &TOKYO_NIGHT,
        ThemeId::Latte => &LATTE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catppuccin_mocha_theme_has_expected_name() {
        let (theme, _palette) = catppuccin_mocha();
        assert_eq!(theme.to_string(), "Catppuccin Mocha");
    }

    #[test]
    fn tokyo_night_storm_theme_has_expected_name() {
        let (theme, _palette) = tokyo_night_storm();
        assert_eq!(theme.to_string(), "Tokyo Night Storm");
    }

    #[test]
    fn latte_theme_has_expected_name() {
        let (theme, _palette) = latte();
        assert_eq!(theme.to_string(), "Latte");
    }

    #[test]
    fn palette_for_theme_returns_correct_palette() {
        assert_eq!(
            palette_for_theme(ThemeId::CatppuccinMocha),
            &CATPPUCCIN_MOCHA
        );
        assert_eq!(palette_for_theme(ThemeId::TokyoNight), &TOKYO_NIGHT);
        assert_eq!(palette_for_theme(ThemeId::Latte), &LATTE);
    }
}
