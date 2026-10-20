use forge_types::{NamesLanguage, current_names_language, set_current_names_language};

#[derive(Debug, Clone, Copy, Default)]
pub struct DateNamesHandle;

impl DateNamesHandle {
    pub fn set_language(&self, language: NamesLanguage) {
        set_current_names_language(language);
    }

    pub fn language(&self) -> NamesLanguage {
        current_names_language()
    }
}
