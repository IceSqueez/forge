use forge_components::{Density, ForgePalette, ThemeId};
use gpui::{App, Global};

/// Active presentation state installed as a gpui `Global` at boot and read by
/// every view-entity at render time. Carries UI-presentation values only (the
/// resolved theme palette and the density scale) — never runtime or domain data —
/// so reading it in render does not breach the component kit's runtime-blindness.
///
/// View-entities read this global and pass `palette` into kit components through
/// the components' existing parameter; the kit itself never reads the global.
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

/// Render-side accessor for the presentation `Global`. Implemented on `App`, so
/// it is reachable from any `Context<_>` through its `Deref<Target = App>` — a
/// view reads the active palette and density inside its `render`.
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
