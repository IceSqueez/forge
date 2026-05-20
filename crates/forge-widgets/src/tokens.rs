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
    Spacious,
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
    Huge,
    Mega,
    Giga,
}

impl Spacing {
    fn base_px(self) -> f32 {
        match self {
            Self::Xs => 4.0,
            Self::Sm => 6.0,
            Self::Md => 8.0,
            Self::Lg => 10.0,
            Self::Xl => 12.0,
            Self::Xxl => 14.0,
            Self::Xxxl => 18.0,
            Self::Huge => 24.0,
            Self::Mega => 32.0,
            Self::Giga => 40.0,
        }
    }
}

/// Returns spacing in logical pixels, density-adjusted and rounded to nearest integer.
pub fn spacing(s: Spacing, d: Density) -> u16 {
    let multiplier = match d {
        Density::Compact => 0.85,
        Density::Cozy => 1.0,
        Density::Spacious => 1.2,
    };
    (s.base_px() * multiplier).round() as u16
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radius {
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
    Xxl,
    Xxxl,
    Hero,
    /// Returns a large sentinel so iced renders a fully rounded pill/circle.
    Pill,
}

pub fn radius(r: Radius) -> f32 {
    match r {
        Radius::Xs => 4.0,
        Radius::Sm => 6.0,
        Radius::Md => 7.0,
        Radius::Lg => 8.0,
        Radius::Xl => 9.0,
        Radius::Xxl => 10.0,
        Radius::Xxxl => 11.0,
        Radius::Hero => 14.0,
        Radius::Pill => 9999.0,
    }
}

pub const BORDER_THIN: f32 = 1.0;
pub const BORDER_ACCENT: f32 = 1.0;

pub const FONT_CAPS_XS: f32 = 9.5;
pub const FONT_CAPS_SM: f32 = 10.5;
pub const FONT_CAPS: f32 = 11.0;
pub const FONT_BODY_SM: f32 = 11.5;
pub const FONT_BODY: f32 = 12.0;
pub const FONT_BODY_MD: f32 = 12.5;
pub const FONT_BODY_LG: f32 = 13.0;
pub const FONT_PLATFORM_NAME: f32 = 13.5;
pub const FONT_VALUE: f32 = 14.0;
pub const FONT_HEADING_SM: f32 = 16.0;
pub const FONT_HEADING: f32 = 18.0;
pub const FONT_PAGE_TITLE: f32 = 20.0;
pub const FONT_HERO: f32 = 22.0;
pub const FONT_DEVICE_CODE: f32 = 28.0;

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

/// Returns bundled Inter + JetBrains Mono font bytes; iced loads these so the
/// UI looks consistent across systems regardless of installed fonts.
pub fn load_fonts() -> Vec<std::borrow::Cow<'static, [u8]>> {
    vec![
        std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
    ]
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
    fn spacing_cozy_returns_html_observed_values() {
        assert_eq!(spacing(Spacing::Xs, Density::Cozy), 4);
        assert_eq!(spacing(Spacing::Sm, Density::Cozy), 6);
        assert_eq!(spacing(Spacing::Md, Density::Cozy), 8);
        assert_eq!(spacing(Spacing::Lg, Density::Cozy), 10);
        assert_eq!(spacing(Spacing::Xl, Density::Cozy), 12);
        assert_eq!(spacing(Spacing::Xxl, Density::Cozy), 14);
        assert_eq!(spacing(Spacing::Xxxl, Density::Cozy), 18);
        assert_eq!(spacing(Spacing::Huge, Density::Cozy), 24);
        assert_eq!(spacing(Spacing::Mega, Density::Cozy), 32);
        assert_eq!(spacing(Spacing::Giga, Density::Cozy), 40);
    }

    #[test]
    fn spacing_compact_scales_down() {
        assert_eq!(spacing(Spacing::Md, Density::Compact), 7);
        assert_eq!(spacing(Spacing::Xxxl, Density::Compact), 15);
    }

    #[test]
    fn spacing_spacious_increases() {
        assert_eq!(spacing(Spacing::Md, Density::Spacious), 10);
        assert_eq!(spacing(Spacing::Xxxl, Density::Spacious), 22);
    }

    #[test]
    fn radius_returns_correct_px_per_variant() {
        assert_eq!(radius(Radius::Xs), 4.0);
        assert_eq!(radius(Radius::Sm), 6.0);
        assert_eq!(radius(Radius::Md), 7.0);
        assert_eq!(radius(Radius::Lg), 8.0);
        assert_eq!(radius(Radius::Xl), 9.0);
        assert_eq!(radius(Radius::Xxl), 10.0);
        assert_eq!(radius(Radius::Xxxl), 11.0);
        assert_eq!(radius(Radius::Hero), 14.0);
    }

    #[test]
    fn radius_pill_renders_as_circle() {
        assert!(radius(Radius::Pill) >= 1000.0);
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
