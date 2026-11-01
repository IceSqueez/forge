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
    use forge_speech_types::{EngineId, VoiceGender, VoiceId};

    use super::*;

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
}
