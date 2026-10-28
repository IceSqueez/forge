use crate::{Density, Presentation, ThemeId};

pub(crate) fn install_presentation(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy)));
}
