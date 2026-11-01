use crate::audio::voice_listing::alphabet_key;
use crate::shared::option_choices::ChoiceOption;
use forge_components::{PickerItem, tr};
use forge_runtime::EngineVoiceRef;
use forge_speech_types::TtsVoice;
use forge_tts_core::engine_display_name;
use gpui::SharedString;

const UNLISTED_STORED_VOICE: &str = "stored";

pub(crate) struct AliasVoiceChoices {
    pub(crate) items: Vec<PickerItem>,
    pub(crate) current: Option<SharedString>,
}

fn display_locale(voice: &TtsVoice) -> String {
    voice.locale.replace('_', "-")
}

pub(crate) fn voice_row_sublabel(voice: &TtsVoice) -> String {
    let engine = engine_display_name(&voice.engine_id.0);
    if voice.locale.is_empty() {
        engine.to_owned()
    } else {
        format!("{engine} - {}", display_locale(voice))
    }
}

fn installed_voice_position(
    voices: &[TtsVoice],
    engine: Option<&str>,
    voice_id: &str,
) -> Option<usize> {
    voices.iter().position(|voice| {
        voice.id.0 == voice_id && engine.is_none_or(|engine| voice.engine_id.0 == engine)
    })
}

pub(crate) fn alias_voice_choices(
    voices: &[TtsVoice],
    engine: Option<&str>,
    stored_voice: &str,
) -> AliasVoiceChoices {
    let mut offered: Vec<(usize, &TtsVoice)> = voices
        .iter()
        .enumerate()
        .filter(|(_, voice)| engine.is_none_or(|engine| voice.engine_id.0 == engine))
        .collect();
    offered
        .sort_by_cached_key(|(_, voice)| (alphabet_key(&voice.name), alphabet_key(&voice.locale)));
    let installed = offered.iter().map(|(index, voice)| PickerItem {
        id: index.to_string().into(),
        label: voice.name.clone().into(),
        sublabel: Some(voice_row_sublabel(voice).into()),
        icon: None,
    });

    let position = installed_voice_position(voices, engine, stored_voice);
    let unlisted = (position.is_none() && !stored_voice.is_empty()).then(|| PickerItem {
        id: UNLISTED_STORED_VOICE.into(),
        label: stored_voice.to_owned().into(),
        sublabel: Some(tr!("tts_aliases_form_voice_not_installed").into()),
        icon: None,
    });
    let current = position
        .map(|position| SharedString::from(position.to_string()))
        .or_else(|| unlisted.as_ref().map(|item| item.id.clone()));
    AliasVoiceChoices {
        items: unlisted.into_iter().chain(installed).collect(),
        current,
    }
}

pub(crate) fn alias_voice_field(
    voices: &[TtsVoice],
    engine: Option<&str>,
    stored_voice: &str,
) -> Option<String> {
    if stored_voice.is_empty() {
        return None;
    }
    let installed = installed_voice_position(voices, engine, stored_voice)
        .and_then(|position| voices.get(position));
    Some(match installed {
        Some(voice) if voice.locale.is_empty() => voice.name.clone(),
        Some(voice) => format!("{} ({})", voice.name, display_locale(voice)),
        None => format!(
            "{stored_voice} - {}",
            tr!("tts_aliases_form_voice_not_installed")
        ),
    })
}

pub(crate) fn engine_voice_options(voices: &[TtsVoice]) -> Vec<ChoiceOption> {
    let mut options: Vec<ChoiceOption> = voices
        .iter()
        .map(|voice| {
            let encoded = EngineVoiceRef::new(voice.engine_id.0.clone(), voice.id.0.clone());
            let engine = engine_display_name(&voice.engine_id.0);
            let label = if voice.locale.is_empty() {
                format!("{engine} - {}", voice.name)
            } else {
                format!("{engine} - {} ({})", voice.name, voice.locale)
            };
            ChoiceOption::new(encoded.to_string(), label)
        })
        .collect();
    options.sort_by(|a, b| a.label.cmp(&b.label));
    options
}

#[cfg(test)]
mod tests {
    use forge_components::item_matches;
    use forge_speech_types::{EngineId, VoiceGender, VoiceId};
    use forge_storage::Language;

    use super::*;
    use crate::app::i18n::install_language;

    fn voice(engine_id: &str, id: &str, name: &str, locale: &str) -> TtsVoice {
        TtsVoice {
            id: VoiceId(id.to_owned()),
            name: name.to_owned(),
            locale: locale.to_owned(),
            gender: VoiceGender::Neutral,
            engine_id: EngineId(engine_id.to_owned()),
            is_neural: false,
            sample_rate_hint: 22_050,
        }
    }

    #[test]
    fn options_encode_the_engine_voice_and_sort_by_their_label() {
        let voices = [
            voice("piper", "en_US/amy", "amy", "en_US"),
            voice("sapi", "zira", "Zira", ""),
            voice("nsspeech", "uk.lesya", "Lesya", "uk-UA"),
        ];

        let options = engine_voice_options(&voices);

        assert_eq!(
            options,
            vec![
                ChoiceOption::new("nsspeech/uk.lesya", "Apple AVSpeech - Lesya (uk-UA)"),
                ChoiceOption::new("sapi/zira", "Microsoft SAPI 5 - Zira"),
                ChoiceOption::new("piper/en_US/amy", "Piper - amy (en_US)"),
            ]
        );
    }

    fn catalog() -> Vec<TtsVoice> {
        vec![
            voice("piper", "en_US/amy", "amy", "en_US"),
            voice("piper", "uk_UA/lada", "Lada", "uk_UA"),
            voice("piper", "de_DE/eva", "Eva", "de-DE"),
            voice("sapi", "zira", "Zira", ""),
        ]
    }

    fn rows(choices: &AliasVoiceChoices) -> Vec<(String, Option<String>)> {
        choices
            .items
            .iter()
            .map(|item| {
                (
                    item.label.to_string(),
                    item.sublabel.as_ref().map(ToString::to_string),
                )
            })
            .collect()
    }

    fn row(label: &str, sublabel: &str) -> (String, Option<String>) {
        (label.to_owned(), Some(sublabel.to_owned()))
    }

    #[test]
    fn the_picker_lists_the_chosen_engine_alphabetically_with_the_locale_in_the_row() {
        let choices = alias_voice_choices(&catalog(), Some("piper"), "uk_UA/lada");

        assert_eq!(
            rows(&choices),
            [
                row("amy", "Piper - en-US"),
                row("Eva", "Piper - de-DE"),
                row("Lada", "Piper - uk-UA"),
            ]
        );
        assert_eq!(choices.current.as_deref(), Some("1"));
    }

    #[test]
    fn without_an_engine_every_voice_is_offered_with_its_engine() {
        let choices = alias_voice_choices(&catalog(), None, "");

        assert_eq!(choices.items.len(), 4);
        assert_eq!(choices.current, None);
        assert_eq!(
            rows(&choices).last(),
            Some(&row("Zira", "Microsoft SAPI 5"))
        );
    }

    #[test]
    fn a_stored_voice_that_is_gone_stays_on_top_as_the_current_entry() {
        install_language(Language::En);
        for (label, engine, stored) in [
            ("uninstalled", Some("piper"), "uk_UA/gone"),
            ("installed on another engine", Some("sapi"), "en_US/amy"),
        ] {
            let choices = alias_voice_choices(&catalog(), engine, stored);

            assert_eq!(
                rows(&choices).first(),
                Some(&row(stored, "Not installed")),
                "{label}"
            );
            assert_eq!(
                choices.current.as_deref(),
                Some(UNLISTED_STORED_VOICE),
                "{label}"
            );
            assert_eq!(
                alias_voice_field(&catalog(), engine, stored),
                Some(format!("{stored} - Not installed")),
                "{label}"
            );
        }
    }

    #[test]
    fn the_field_names_an_installed_voice_and_stays_blank_without_one() {
        for (stored, expected) in [
            ("uk_UA/lada", Some("Lada (uk-UA)")),
            ("zira", Some("Zira")),
            ("", None),
        ] {
            assert_eq!(
                alias_voice_field(&catalog(), None, stored).as_deref(),
                expected,
                "{stored:?}"
            );
        }
    }

    #[test]
    fn search_finds_a_voice_by_name_or_locale_in_any_case() {
        let choices = alias_voice_choices(&catalog(), Some("piper"), "");
        for (query, expected) in [
            ("uk-UA", vec!["Lada"]),
            ("uk-ua", vec!["Lada"]),
            ("lad", vec!["Lada"]),
            ("de", vec!["Eva"]),
            ("fr-FR", vec![]),
        ] {
            let found: Vec<&str> = choices
                .items
                .iter()
                .filter(|item| item_matches(&item.label, item.sublabel.as_deref(), query))
                .map(|item| item.label.as_ref())
                .collect();
            assert_eq!(found, expected, "{query}");
        }
    }
}
