use forge_tts_core::{EngineId, TtsVoice, VoiceGender, VoiceId};

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn lcid_to_bcp47(lcid: u32) -> &'static str {
    match lcid {
        0x0401 => "ar-SA",
        0x0402 => "bg-BG",
        0x0404 => "zh-TW",
        0x0405 => "cs-CZ",
        0x0406 => "da-DK",
        0x0407 => "de-DE",
        0x0807 => "de-CH",
        0x0C07 => "de-AT",
        0x0408 => "el-GR",
        0x0409 => "en-US",
        0x0809 => "en-GB",
        0x0C09 => "en-AU",
        0x1009 => "en-CA",
        0x040A => "es-ES",
        0x080A => "es-MX",
        0x040B => "fi-FI",
        0x040C => "fr-FR",
        0x080C => "fr-BE",
        0x0C0C => "fr-CA",
        0x040D => "he-IL",
        0x040E => "hu-HU",
        0x040F => "is-IS",
        0x0410 => "it-IT",
        0x0810 => "it-CH",
        0x0411 => "ja-JP",
        0x0412 => "ko-KR",
        0x0413 => "nl-NL",
        0x0414 => "nb-NO",
        0x0415 => "pl-PL",
        0x0416 => "pt-BR",
        0x0816 => "pt-PT",
        0x0418 => "ro-RO",
        0x0419 => "ru-RU",
        0x041A => "hr-HR",
        0x041B => "sk-SK",
        0x041D => "sv-SE",
        0x041F => "tr-TR",
        0x0422 => "uk-UA",
        0x0424 => "sl-SI",
        0x0425 => "et-EE",
        0x0426 => "lv-LV",
        0x0427 => "lt-LT",
        0x0429 => "fa-IR",
        0x0804 => "zh-CN",
        _ => "und",
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn parse_sapi_hex_locale(hex_str: &str) -> String {
    if let Ok(lcid) = u32::from_str_radix(hex_str, 16) {
        let tag = lcid_to_bcp47(lcid);
        if tag != "und" {
            return tag.to_owned();
        }
    }
    hex_str.to_owned()
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn parse_gender(s: &str) -> VoiceGender {
    match s {
        "Male" => VoiceGender::Male,
        "Female" => VoiceGender::Female,
        _ => VoiceGender::Neutral,
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn build_tts_voice(
    id_str: String,
    name: String,
    locale_hex: &str,
    gender_str: &str,
    engine_id: &EngineId,
) -> TtsVoice {
    TtsVoice {
        id: VoiceId(id_str),
        name,
        locale: parse_sapi_hex_locale(locale_hex),
        gender: parse_gender(gender_str),
        engine_id: engine_id.clone(),
        is_neural: false,
        sample_rate_hint: 22_050,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcid_to_bcp47_maps_known_locales_and_falls_back_to_und() {
        for (lcid, expected) in [
            (0x0409, "en-US"),
            (0x0419, "ru-RU"),
            (0x0422, "uk-UA"),
            (0xFFFF, "und"),
        ] {
            assert_eq!(lcid_to_bcp47(lcid), expected, "lcid={lcid:#x}");
        }
    }

    #[test]
    fn parse_sapi_hex_locale_returns_bcp47_or_verbatim() {
        assert_eq!(parse_sapi_hex_locale("409"), "en-US");
        assert_eq!(parse_sapi_hex_locale("FFFF"), "FFFF");
        assert_eq!(parse_sapi_hex_locale("xyz"), "xyz");
    }

    #[test]
    fn parse_gender_variants() {
        assert!(matches!(parse_gender("Male"), VoiceGender::Male));
        assert!(matches!(parse_gender("Female"), VoiceGender::Female));
        assert!(matches!(parse_gender("Neutral"), VoiceGender::Neutral));
        assert!(matches!(parse_gender(""), VoiceGender::Neutral));
    }

    #[test]
    fn build_tts_voice_sets_fields() {
        let eid = EngineId("sapi".into());
        let v = build_tts_voice("token-id".into(), "Test Voice".into(), "409", "Male", &eid);
        assert_eq!(v.id.0, "token-id");
        assert_eq!(v.name, "Test Voice");
        assert_eq!(v.locale, "en-US");
        assert!(matches!(v.gender, VoiceGender::Male));
        assert!(!v.is_neural);
        assert_eq!(v.sample_rate_hint, 22_050);
    }
}
