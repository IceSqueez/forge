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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_names_handle_reads_back_each_language_it_sets() {
        let handle = DateNamesHandle;
        for language in [NamesLanguage::Uk, NamesLanguage::En] {
            handle.set_language(language);
            assert_eq!(handle.language(), language);
        }
    }
}
