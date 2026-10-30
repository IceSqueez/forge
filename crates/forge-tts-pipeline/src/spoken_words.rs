use forge_speech_types::LanguageCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpokenWords {
    pub link: &'static str,
    pub says: &'static str,
}

const ENGLISH: SpokenWords = SpokenWords {
    link: "link",
    says: "says",
};

const UKRAINIAN: SpokenWords = SpokenWords {
    link: "посилання",
    says: "каже",
};

const UKRAINIAN_CODE: &str = "uk";

pub fn spoken_words(language: Option<LanguageCode>) -> SpokenWords {
    match language {
        Some(code) if code.to_string() == UKRAINIAN_CODE => UKRAINIAN,
        _ => ENGLISH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ukrainian_voice_speaks_ukrainian_words() {
        let words = spoken_words(LanguageCode::from_locale("uk-UA"));
        assert_eq!((words.says, words.link), ("каже", "посилання"));
    }

    #[test]
    fn english_and_unknown_and_missing_languages_speak_english_words() {
        for language in [
            LanguageCode::from_locale("en-GB"),
            LanguageCode::from_locale("de"),
            None,
        ] {
            let words = spoken_words(language);
            assert_eq!((words.says, words.link), ("says", "link"), "{language:?}");
        }
    }
}
