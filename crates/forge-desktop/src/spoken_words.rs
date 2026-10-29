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
