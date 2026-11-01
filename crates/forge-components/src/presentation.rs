use crate::tokens::{DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY};
use crate::{AccentColor, Density, ForgePalette, ThemeId};
use gpui::{App, Global, SharedString};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFamilies {
    pub body: SharedString,
    pub mono: SharedString,
}

impl FontFamilies {
    pub fn with_overrides(body: Option<SharedString>, mono: Option<SharedString>) -> Self {
        Self {
            body: body.unwrap_or_else(default_body_family),
            mono: mono.unwrap_or_else(default_mono_family),
        }
    }

    pub fn set_body_override(&mut self, family: Option<SharedString>) {
        self.body = family.unwrap_or_else(default_body_family);
    }

    pub fn set_mono_override(&mut self, family: Option<SharedString>) {
        self.mono = family.unwrap_or_else(default_mono_family);
    }
}

impl Default for FontFamilies {
    fn default() -> Self {
        Self::with_overrides(None, None)
    }
}

fn default_body_family() -> SharedString {
    SharedString::from(DEFAULT_BODY_FAMILY)
}

fn default_mono_family() -> SharedString {
    SharedString::from(DEFAULT_MONO_FAMILY)
}

pub struct Presentation {
    pub theme: ThemeId,
    pub accent: AccentColor,
    pub palette: ForgePalette,
    pub density: Density,
    pub fonts: FontFamilies,
}

impl Presentation {
    pub fn new(theme: ThemeId, density: Density) -> Self {
        Self::accented(theme, AccentColor::default(), density)
    }

    pub fn accented(theme: ThemeId, accent: AccentColor, density: Density) -> Self {
        Self {
            theme,
            accent,
            palette: theme.accented_palette(accent),
            density,
            fonts: FontFamilies::default(),
        }
    }

    pub fn with_fonts(mut self, fonts: FontFamilies) -> Self {
        self.fonts = fonts;
        self
    }

    pub fn restyled(&self, theme: ThemeId, accent: AccentColor, density: Density) -> Self {
        Self::accented(theme, accent, density).with_fonts(self.fonts.clone())
    }
}

impl Global for Presentation {}

pub trait ActivePresentation {
    fn palette(&self) -> ForgePalette;
    fn density(&self) -> Density;
    fn theme(&self) -> ThemeId;
    fn accent(&self) -> AccentColor;
    fn body_family(&self) -> SharedString;
    fn mono_family(&self) -> SharedString;
}

impl ActivePresentation for App {
    fn palette(&self) -> ForgePalette {
        self.global::<Presentation>().palette
    }

    fn density(&self) -> Density {
        self.global::<Presentation>().density
    }

    fn theme(&self) -> ThemeId {
        self.global::<Presentation>().theme
    }

    fn accent(&self) -> AccentColor {
        self.global::<Presentation>().accent
    }

    fn body_family(&self) -> SharedString {
        self.global::<Presentation>().fonts.body.clone()
    }

    fn mono_family(&self) -> SharedString {
        self.global::<Presentation>().fonts.mono.clone()
    }
}
