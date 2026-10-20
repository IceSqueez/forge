use std::sync::atomic::{AtomicU8, Ordering};

use super::NamesLanguage;

const EN_SLOT: u8 = 0;
const UK_SLOT: u8 = 1;

static CURRENT_NAMES_LANGUAGE: AtomicU8 = AtomicU8::new(EN_SLOT);

pub fn current_names_language() -> NamesLanguage {
    match CURRENT_NAMES_LANGUAGE.load(Ordering::Acquire) {
        UK_SLOT => NamesLanguage::Uk,
        _ => NamesLanguage::En,
    }
}

pub fn set_current_names_language(language: NamesLanguage) {
    let slot = match language {
        NamesLanguage::En => EN_SLOT,
        NamesLanguage::Uk => UK_SLOT,
    };
    CURRENT_NAMES_LANGUAGE.store(slot, Ordering::Release);
}
