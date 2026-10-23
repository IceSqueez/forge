use forge_tts_core::{EngineId, TtsVoice, VoiceGender, VoiceId};

use crate::synth::SAMPLE_RATE_HZ;

const UNDETERMINED_LOCALE: &str = "und";
const LANGUAGE_LIST_SEPARATOR: char = ';';

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
        _ => UNDETERMINED_LOCALE,
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn primary_language_tag(language_attribute: &str) -> Option<&'static str> {
    language_attribute
        .split(LANGUAGE_LIST_SEPARATOR)
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .filter_map(|segment| u32::from_str_radix(segment, 16).ok())
        .map(lcid_to_bcp47)
        .find(|tag| *tag != UNDETERMINED_LOCALE)
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn parse_sapi_hex_locale(language_attribute: &str) -> String {
    if let Some(tag) = primary_language_tag(language_attribute) {
        return tag.to_owned();
    }
    let trimmed = language_attribute.trim();
    if trimmed.is_empty() {
        UNDETERMINED_LOCALE.to_owned()
    } else {
        trimmed.to_owned()
    }
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
        sample_rate_hint: SAMPLE_RATE_HZ,
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
    fn parse_sapi_hex_locale_maps_first_known_lcid_or_keeps_input() {
        for (attribute, expected) in [
            ("409", "en-US"),
            ("409;9", "en-US"),
            ("FFFF;409", "en-US"),
            ("9;422", "uk-UA"),
            (" 419 ", "ru-RU"),
            ("", "und"),
            ("   ", "und"),
            ("FFFF", "FFFF"),
            (" xyz ", "xyz"),
        ] {
            assert_eq!(
                parse_sapi_hex_locale(attribute),
                expected,
                "attribute={attribute:?}"
            );
        }
    }

    #[test]
    fn primary_language_tag_is_none_without_a_known_lcid() {
        for attribute in ["", ";", "FFFF", "9", "xyz;FFFF"] {
            assert_eq!(
                primary_language_tag(attribute),
                None,
                "attribute={attribute:?}"
            );
        }
    }

    #[test]
    fn parse_gender_variants() {
        assert!(matches!(parse_gender("Male"), VoiceGender::Male));
        assert!(matches!(parse_gender("Female"), VoiceGender::Female));
        assert!(matches!(parse_gender("Neutral"), VoiceGender::Neutral));
        assert!(matches!(parse_gender(""), VoiceGender::Neutral));
    }
}
