use crate::option_choices::ChoiceOption;
use forge_runtime::EngineVoiceRef;
use forge_speech_types::TtsVoice;
use forge_tts_core::engine_display_name;

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
