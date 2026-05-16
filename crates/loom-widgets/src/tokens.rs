use iced::{Font, font};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ThemeId {
    #[default]
    CatppuccinMocha,
    TokyoNight,
    Latte,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Density {
    Compact,
    #[default]
    Cozy,
    Comfortable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spacing {
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
    Xxl,
    Xxxl,
}

impl Spacing {
    fn base_px(self) -> f32 {
        match self {
            Self::Xs => 4.0,
            Self::Sm => 6.0,
            Self::Md => 8.0,
            Self::Lg => 10.0,
            Self::Xl => 14.0,
            Self::Xxl => 18.0,
            Self::Xxxl => 24.0,
        }
    }
}

/// Returns spacing in logical pixels, density-adjusted and rounded to nearest integer.
pub fn spacing(s: Spacing, d: Density) -> u16 {
    let multiplier = match d {
        Density::Compact => 0.85,
        Density::Cozy => 1.0,
        Density::Comfortable => 1.2,
    };
    (s.base_px() * multiplier).round() as u16
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontRole {
    Body,
    Monospace,
}

/// Caller must invoke `load_fonts()` at startup; otherwise iced falls back to system fonts.
pub fn font(role: FontRole) -> Font {
    match role {
        FontRole::Body => Font::with_name("Inter"),
        FontRole::Monospace => Font {
            family: font::Family::Name("JetBrains Mono"),
            weight: font::Weight::Normal,
            stretch: font::Stretch::Normal,
            style: font::Style::Normal,
        },
    }
}

/// Empty when the `bundled-fonts` feature is off; iced then uses system fonts.
pub fn load_fonts() -> Vec<std::borrow::Cow<'static, [u8]>> {
    #[cfg(feature = "bundled-fonts")]
    {
        vec![
            std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.otf")),
            std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
        ]
    }
    #[cfg(not(feature = "bundled-fonts"))]
    {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_id_default_is_catppuccin_mocha() {
        assert_eq!(ThemeId::default(), ThemeId::CatppuccinMocha);
    }

    #[test]
    fn density_default_is_cozy() {
        assert_eq!(Density::default(), Density::Cozy);
    }

    #[test]
    fn spacing_cozy_returns_documented_values() {
        assert_eq!(spacing(Spacing::Xs, Density::Cozy), 4);
        assert_eq!(spacing(Spacing::Sm, Density::Cozy), 6);
        assert_eq!(spacing(Spacing::Md, Density::Cozy), 8);
        assert_eq!(spacing(Spacing::Lg, Density::Cozy), 10);
        assert_eq!(spacing(Spacing::Xl, Density::Cozy), 14);
        assert_eq!(spacing(Spacing::Xxl, Density::Cozy), 18);
        assert_eq!(spacing(Spacing::Xxxl, Density::Cozy), 24);
    }

    #[test]
    fn spacing_compact_halves_approximately() {
        assert_eq!(spacing(Spacing::Md, Density::Compact), 7);
        assert_eq!(spacing(Spacing::Xxxl, Density::Compact), 20);
    }

    #[test]
    fn spacing_comfortable_increases() {
        assert_eq!(spacing(Spacing::Md, Density::Comfortable), 10);
        assert_eq!(spacing(Spacing::Xxxl, Density::Comfortable), 29);
    }

    #[test]
    fn font_role_body_is_inter() {
        let f = font(FontRole::Body);
        assert_eq!(f.family, iced::font::Family::Name("Inter"));
    }

    #[test]
    fn font_role_monospace_is_jetbrains() {
        let f = font(FontRole::Monospace);
        assert_eq!(f.family, iced::font::Family::Name("JetBrains Mono"));
    }
}
