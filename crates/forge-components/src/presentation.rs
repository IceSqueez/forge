use crate::{AccentColor, Density, ForgePalette, ThemeId};
use gpui::{App, Global};

pub struct Presentation {
    pub theme: ThemeId,
    pub accent: AccentColor,
    pub palette: ForgePalette,
    pub density: Density,
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
        }
    }
}

impl Global for Presentation {}

pub trait ActivePresentation {
    fn palette(&self) -> ForgePalette;
    fn density(&self) -> Density;
    fn theme(&self) -> ThemeId;
    fn accent(&self) -> AccentColor;
}

impl ActivePresentation for App {
    fn palette(&self) -> ForgePalette {
        self.global::<Presentation>().palette
    }

    fn density(&self) -> Density {
        self.try_global::<Presentation>()
            .map_or_else(Density::default, |presentation| presentation.density)
    }

    fn theme(&self) -> ThemeId {
        self.global::<Presentation>().theme
    }

    fn accent(&self) -> AccentColor {
        self.global::<Presentation>().accent
    }
}
