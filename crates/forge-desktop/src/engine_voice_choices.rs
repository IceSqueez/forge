use forge_runtime::EngineVoiceRef;
use forge_tts_core::TtsVoice;

pub(crate) fn engine_voice_options(voices: &[TtsVoice]) -> Vec<(String, String)> {
    let mut options: Vec<(String, String)> = voices
        .iter()
        .map(|voice| {
            let encoded = EngineVoiceRef::new(voice.engine_id.0.clone(), voice.id.0.clone());
            let engine = voice.engine_id.display_name();
            let label = if voice.locale.is_empty() {
                format!("{engine} - {}", voice.name)
            } else {
                format!("{engine} - {} ({})", voice.name, voice.locale)
            };
            (encoded.to_string(), label)
        })
        .collect();
    options.sort_by(|a, b| a.1.cmp(&b.1));
    options
}

#[cfg(test)]
mod tests {
    use forge_tts_core::{EngineId, VoiceGender, VoiceId};

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
                (
                    "nsspeech/uk.lesya".to_owned(),
                    "Apple AVSpeech - Lesya (uk-UA)".to_owned()
                ),
                ("sapi/zira".to_owned(), "Microsoft SAPI 5 - Zira".to_owned()),
                (
                    "piper/en_US/amy".to_owned(),
                    "Piper - amy (en_US)".to_owned()
                ),
            ]
        );
    }
}
