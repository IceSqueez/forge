use forge_components::{Density, ForgePalette, ThemeId};
use forge_storage::Language;
use gpui::{App, Global};

pub struct ActiveLanguage(pub Language);

impl Global for ActiveLanguage {}

pub struct Presentation {
    pub theme: ThemeId,
    pub palette: ForgePalette,
    pub density: Density,
}

impl Presentation {
    pub fn new(theme: ThemeId, density: Density) -> Self {
        Self {
            theme,
            palette: theme.palette(),
            density,
        }
    }
}

impl Global for Presentation {}

pub trait ActivePresentation {
    fn palette(&self) -> ForgePalette;
    fn density(&self) -> Density;
    fn theme(&self) -> ThemeId;
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
}
