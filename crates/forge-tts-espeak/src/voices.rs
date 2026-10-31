use forge_speech_types::{EngineId, TtsVoice, VoiceGender, VoiceId};

pub(crate) fn parse_voices_output(raw: &str, engine_id: &EngineId) -> Vec<TtsVoice> {
    raw.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.is_empty() && !trimmed.starts_with("Pty")
        })
        .filter_map(|line| parse_voice_line(line, engine_id))
        .collect()
}

fn parse_voice_line(line: &str, engine_id: &EngineId) -> Option<TtsVoice> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 5 {
        return None;
    }

    let language_code = tokens[1];

    let gender_str = tokens[2].rsplit('/').next().unwrap_or_default();
    let voice_name = tokens[3];
    let file_path = tokens[4];

    let gender = match gender_str {
        "M" => VoiceGender::Male,
        "F" => VoiceGender::Female,
        _ => VoiceGender::Neutral,
    };

    Some(TtsVoice {
        id: VoiceId(file_path.to_owned()),
        name: voice_name.replace('_', " "),
        locale: language_code.replace('_', "-"),
        gender,
        engine_id: engine_id.clone(),
        is_neural: false,
        sample_rate_hint: 22_050,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VOICES_FIXTURE: &str = "\
Pty Language       Age/Gender VoiceName          File                 Other Languages
 5  af              --/M      Afrikaans          gmw/af
 5  en-gb           --/M      English_(Great_Britain) gmw/en             (en 2)
 5  en-us           --/M      English_(America)  gmw/en-US          (en 3)(en 2)
 5  uk              --/M      Ukrainian          zle/uk
 5  cmn             --/M      Chinese_(Mandarin,_latin_as_English) sit/cmn            (zh-cmn 5)(zh 5)
 5  fr-fr           --/F      French_(France)    roa/fr             (fr 5)
 5  eo              --/-      Esperanto          art/eo
";

    fn engine_id() -> EngineId {
        EngineId("espeak-ng".into())
    }

    fn find<'a>(voices: &'a [TtsVoice], id: &str) -> &'a TtsVoice {
        voices
            .iter()
            .find(|v| v.id.0 == id)
            .unwrap_or_else(|| panic!("voice {id} missing"))
    }

    #[test]
    fn parse_skips_the_header_and_yields_one_voice_per_row() {
        let voices = parse_voices_output(VOICES_FIXTURE, &engine_id());
        assert_eq!(voices.len(), 7);
    }

    #[test]
    fn parse_reads_gender_from_the_slash_separated_age_gender_column() {
        let voices = parse_voices_output(VOICES_FIXTURE, &engine_id());
        assert!(matches!(find(&voices, "gmw/af").gender, VoiceGender::Male));
        assert!(matches!(
            find(&voices, "roa/fr").gender,
            VoiceGender::Female
        ));
        assert!(matches!(
            find(&voices, "art/eo").gender,
            VoiceGender::Neutral
        ));
    }

    #[test]
    fn parse_uses_the_file_column_as_voice_id_and_ignores_other_languages() {
        let voices = parse_voices_output(VOICES_FIXTURE, &engine_id());
        assert_eq!(find(&voices, "gmw/en").locale, "en-gb");
        assert_eq!(find(&voices, "sit/cmn").locale, "cmn");
    }

    #[test]
    fn parse_turns_underscores_in_the_voice_name_into_spaces() {
        let voices = parse_voices_output(VOICES_FIXTURE, &engine_id());
        assert_eq!(find(&voices, "gmw/en-US").name, "English (America)");
    }

    #[test]
    fn parse_normalises_underscore_language_codes_to_bcp47() {
        let raw = "Pty Language Age/Gender VoiceName File\n 5  en_us  --/M  english-us  en/en-us";
        let voices = parse_voices_output(raw, &engine_id());
        assert_eq!(voices[0].locale, "en-us");
    }

    #[test]
    fn parse_drops_rows_with_fewer_than_five_columns() {
        assert!(parse_voices_output("  5  af  --/M  Afrikaans", &engine_id()).is_empty());
    }
}
