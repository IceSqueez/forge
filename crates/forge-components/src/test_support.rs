use std::rc::Rc;

use fluent::{FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

use crate::locale::{CatalogLanguage, catalog_source, install_bundle, set_thread_language};
use crate::{Density, Presentation, ThemeId};

pub(crate) fn install_presentation(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy)));
}

pub(crate) fn install_source(source: &str) {
    let id: LanguageIdentifier = "en-US".parse().unwrap();
    let resource = FluentResource::try_new(source.to_owned()).unwrap();
    let mut bundle = FluentBundle::new(vec![id]);
    bundle.set_use_isolating(false);
    bundle.add_resource(resource).unwrap();
    install_bundle(Rc::new(bundle));
}

pub(crate) fn install_catalog(language: CatalogLanguage) {
    install_source(catalog_source(language));
    set_thread_language(language);
}
