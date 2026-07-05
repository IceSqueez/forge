use iced::Theme;
use iced::theme::Palette;

use crate::palette::{CATPPUCCIN_MOCHA, ForgePalette, LATTE, TOKYO_NIGHT};
use crate::tokens::ThemeId;

pub fn catppuccin_mocha() -> (Theme, ForgePalette) {
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

pub fn tokyo_night_storm() -> (Theme, ForgePalette) {
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

pub fn latte() -> (Theme, ForgePalette) {
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

pub fn palette_for_theme(theme_id: ThemeId) -> &'static ForgePalette {
    match theme_id {
        ThemeId::CatppuccinMocha => &CATPPUCCIN_MOCHA,
        ThemeId::TokyoNight => &TOKYO_NIGHT,
        ThemeId::Latte => &LATTE,
    }
}

pub fn theme_assets(theme_id: ThemeId) -> (Theme, ForgePalette) {
    match theme_id {
        ThemeId::CatppuccinMocha => catppuccin_mocha(),
        ThemeId::TokyoNight => tokyo_night_storm(),
        ThemeId::Latte => latte(),
    }
}
