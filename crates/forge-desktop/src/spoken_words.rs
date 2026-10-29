use forge_components::tr;
use forge_speak_queue::SpokenWords;
use forge_storage::Language;

use crate::i18n::message_in;

const LINK_KEY: &str = "tts_spoken_link";
const SAYS_KEY: &str = "tts_spoken_says";

pub fn spoken_words_in(language: Language) -> SpokenWords {
    SpokenWords {
        link: message_in(language, LINK_KEY),
        says: message_in(language, SAYS_KEY),
    }
}

pub fn active_spoken_words() -> SpokenWords {
    SpokenWords {
        link: tr!(LINK_KEY),
        says: tr!(SAYS_KEY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_resolves_both_spoken_words_from_its_catalog() {
        for language in [Language::En, Language::Uk] {
            let words = spoken_words_in(language);
            assert_eq!(
                (
                    words.link.is_empty() || words.link == LINK_KEY,
                    words.says.is_empty() || words.says == SAYS_KEY,
                ),
                (false, false),
                "{language:?}: {words:?}"
            );
        }
    }
}
