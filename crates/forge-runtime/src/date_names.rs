use forge_types::{NamesLanguage, Shared};

#[derive(Debug, Clone, Default)]
pub struct DateNamesHandle(Shared<NamesLanguage>);

impl DateNamesHandle {
    pub fn new(language: NamesLanguage) -> Self {
        Self(Shared::new(language))
    }

    pub fn set_language(&self, language: NamesLanguage) {
        self.0.store(language);
    }

    pub fn language(&self) -> NamesLanguage {
        *self.0.load()
    }
}
