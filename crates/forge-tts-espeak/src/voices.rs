use forge_tts_core::{EngineId, TtsVoice, VoiceGender, VoiceId};

pub fn parse_voices_output(raw: &str, engine_id: &EngineId) -> Vec<TtsVoice> {
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

    let (gender_str, voice_name, file_path) = if tokens[2].parse::<u32>().is_ok() {
        if tokens.len() < 6 {
            return None;
        }
        (tokens[3], tokens[4], tokens[5])
    } else {
        (tokens[2], tokens[3], tokens[4])
    };

    let gender = match gender_str {
        "M" => VoiceGender::Male,
        "F" => VoiceGender::Female,
        _ => VoiceGender::Neutral,
    };

    Some(TtsVoice {
        id: VoiceId(file_path.to_owned()),
        name: voice_name.to_owned(),
        locale: language_code.replace('_', "-"),
        gender,
        engine_id: engine_id.clone(),
        is_neural: false,
        sample_rate_hint: 22_050,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    const VOICES_FIXTURE: &str = "\
 Pty  Language       Age/Gender VoiceName          File          Other Languages
  5  af             M  afrikaans            other/af
  5  de             M  german               europe/de       (de 5)
  5  el             M  greek                europe/el
  5  en             M  english              en/en
  5  en-gb         30 M  english             en/en-gb        (en 5)
  7  en-us          M  english-us           en/en-us
  5  es             M  spanish              europe/es
  5  fr             F  french               europe/fr
  5  hi             M  hindi                asia/hi
  5  it             M  italian              europe/it
  5  ja             M  japanese             asia/ja
  5  ko             M  korean               asia/ko
  5  pt-br          M  portuguese-brazil    pt/pt-br
  5  ru             M  russian              europe/ru
  5  uk             M  ukrainian            europe/uk
  5  zh             M  mandarin             asia/zh";

    fn engine_id() -> EngineId {
        EngineId("espeak-ng".into())
    }

    #[test]
    fn parse_returns_expected_count() {
        let voices = parse_voices_output(VOICES_FIXTURE, &engine_id());
        assert_eq!(voices.len(), 16);
    }

    #[test]
    fn parse_voice_id_is_file_path_token() {
        let voices = parse_voices_output(VOICES_FIXTURE, &engine_id());
        let af = voices
            .iter()
            .find(|v| v.id.0 == "other/af")
            .expect("af voice");
        assert_eq!(af.locale, "af");
        assert_eq!(af.name, "afrikaans");
        assert!(matches!(af.gender, VoiceGender::Male));
    }

    #[test]
    fn parse_age_present_uses_correct_file_path() {
        let voices = parse_voices_output(VOICES_FIXTURE, &engine_id());
        let en_gb = voices
            .iter()
            .find(|v| v.id.0 == "en/en-gb")
            .expect("en/en-gb voice");
        assert_eq!(en_gb.locale, "en-gb");
        assert_eq!(en_gb.name, "english");
        assert!(matches!(en_gb.gender, VoiceGender::Male));
    }

    #[test]
    fn parse_female_gender() {
        let voices = parse_voices_output(VOICES_FIXTURE, &engine_id());
        let fr = voices
            .iter()
            .find(|v| v.id.0 == "europe/fr")
            .expect("french voice");
        assert!(matches!(fr.gender, VoiceGender::Female));
    }

    #[test]
    fn parse_bcp47_locale_normalization() {
        let raw = " Pty  Language       Age/Gender VoiceName          File\n  5  en_us          M  english-us           en/en-us";
        let voices = parse_voices_output(raw, &engine_id());
        assert_eq!(voices[0].locale, "en-us");
    }

    #[test]
    fn parse_sample_rate_hint_always_22050() {
        let voices = parse_voices_output(VOICES_FIXTURE, &engine_id());
        assert!(voices.iter().all(|v| v.sample_rate_hint == 22_050));
    }

    #[test]
    fn parse_engine_id_attached_to_all_voices() {
        let id = engine_id();
        let voices = parse_voices_output(VOICES_FIXTURE, &id);
        assert!(voices.iter().all(|v| v.engine_id == id));
    }

    #[test]
    fn parse_skips_header_line() {
        let header_only =
            " Pty  Language       Age/Gender VoiceName          File          Other Languages\n";
        let voices = parse_voices_output(header_only, &engine_id());
        assert!(voices.is_empty());
    }

    #[test]
    fn parse_skips_short_lines() {
        let raw = "  5  af";
        let voices = parse_voices_output(raw, &engine_id());
        assert!(voices.is_empty());
    }

    #[test]
    fn parse_korean_voice_locale() {
        let voices = parse_voices_output(VOICES_FIXTURE, &engine_id());
        let ko = voices
            .iter()
            .find(|v| v.id.0 == "asia/ko")
            .expect("korean voice");
        assert_eq!(ko.locale, "ko");
    }

    #[test]
    fn parse_pt_br_locale_with_hyphen() {
        let voices = parse_voices_output(VOICES_FIXTURE, &engine_id());
        let pt = voices
            .iter()
            .find(|v| v.id.0 == "pt/pt-br")
            .expect("pt-br voice");
        assert_eq!(pt.locale, "pt-br");
    }
}
