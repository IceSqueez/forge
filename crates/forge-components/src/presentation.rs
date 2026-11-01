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

#[cfg(test)]
mod tests {
    use super::*;

    fn family(name: &'static str) -> Option<SharedString> {
        Some(SharedString::from(name))
    }

    #[test]
    fn an_override_replaces_only_its_own_family() {
        let body_only = FontFamilies::with_overrides(family("Inter"), None);
        let mono_only = FontFamilies::with_overrides(None, family("Hack"));

        assert_eq!(
            (body_only.body.as_ref(), body_only.mono.as_ref()),
            ("Inter", DEFAULT_MONO_FAMILY)
        );
        assert_eq!(
            (mono_only.body.as_ref(), mono_only.mono.as_ref()),
            (DEFAULT_BODY_FAMILY, "Hack")
        );
    }

    #[test]
    fn clearing_an_override_restores_that_familys_default_and_keeps_the_other() {
        let mut fonts = FontFamilies::with_overrides(family("Inter"), family("Hack"));

        fonts.set_body_override(None);
        assert_eq!(
            (fonts.body.as_ref(), fonts.mono.as_ref()),
            (DEFAULT_BODY_FAMILY, "Hack")
        );

        fonts.set_body_override(family("Lora"));
        fonts.set_mono_override(None);
        assert_eq!(
            (fonts.body.as_ref(), fonts.mono.as_ref()),
            ("Lora", DEFAULT_MONO_FAMILY)
        );
    }

    #[test]
    fn restyling_applies_the_new_look_and_keeps_the_chosen_fonts() {
        let fonts = FontFamilies::with_overrides(family("Inter"), family("Hack"));
        let current =
            Presentation::new(ThemeId::ForgeDefault, Density::Cozy).with_fonts(fonts.clone());

        let restyled = current.restyled(ThemeId::Latte, AccentColor::Green, Density::Compact);

        assert_eq!(restyled.fonts, fonts);
        assert_eq!(
            (restyled.theme, restyled.accent, restyled.density),
            (ThemeId::Latte, AccentColor::Green, Density::Compact)
        );
        assert_eq!(
            restyled.palette,
            ThemeId::Latte.accented_palette(AccentColor::Green)
        );
    }
}
