use forge_speech_types::{LanguageCode, TtsVoice};

pub(crate) fn voice_speaks_language(voice: &TtsVoice, language: LanguageCode) -> bool {
    LanguageCode::from_locale(&voice.locale) == Some(language)
}

pub(crate) fn candidate_languages(catalog: &[TtsVoice]) -> Vec<LanguageCode> {
    let mut languages = Vec::new();
    for voice in catalog {
        if let Some(code) = LanguageCode::from_locale(&voice.locale)
            && !languages.contains(&code)
        {
            languages.push(code);
        }
    }
    languages
}

#[cfg(test)]
mod tests {
    use forge_speech_types::{EngineId, VoiceGender, VoiceId};

    use super::*;

    fn make_voice(id: &str, locale: &str) -> TtsVoice {
        TtsVoice {
            id: VoiceId(id.into()),
            name: id.into(),
            locale: locale.into(),
            gender: VoiceGender::Neutral,
            engine_id: EngineId("piper".into()),
            is_neural: false,
            sample_rate_hint: 22_050,
        }
    }

    fn language(code: &str) -> LanguageCode {
        LanguageCode::from_locale(code).unwrap()
    }

    #[test]
    fn voice_speaks_language_compares_primary_subtags_across_engine_locale_shapes() {
        let uk = language("uk");
        for locale in ["uk-UA", "uk_ua", "UK", "uk"] {
            assert!(
                voice_speaks_language(&make_voice("v", locale), uk),
                "locale {locale} serves uk"
            );
        }
        assert!(!voice_speaks_language(&make_voice("v", "en-US"), uk));
    }

    #[test]
    fn voice_with_an_unreadable_locale_serves_no_language_at_all() {
        for locale in ["", "und", "0409", "fil-PH"] {
            let voice = make_voice("mystery", locale);
            for code in ["uk", "en", "ru"] {
                assert!(
                    !voice_speaks_language(&voice, language(code)),
                    "locale {locale:?} must not claim {code}"
                );
            }
        }
    }

    #[test]
    fn candidate_languages_lists_each_language_once_in_catalog_order() {
        let catalog = vec![
            make_voice("a", "en-US"),
            make_voice("b", "uk-UA"),
            make_voice("c", "en-GB"),
            make_voice("d", ""),
            make_voice("e", "und"),
            make_voice("f", "ru_RU"),
        ];
        let codes: Vec<String> = candidate_languages(&catalog)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(codes, vec!["en", "uk", "ru"]);
    }
}
