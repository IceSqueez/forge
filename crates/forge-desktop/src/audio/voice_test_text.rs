use forge_components::tr;
use forge_speak_queue::{EmoteTokenSet, Priority, RequestId, SpeakRequest};
use forge_speech_types::{EngineId, VoiceId};
use forge_storage::DEFAULT_LONGER_THAN_MAX_CHARS;

pub(crate) const VOICE_TEST_TEXT_MAX_CHARS: usize = DEFAULT_LONGER_THAN_MAX_CHARS as usize;

pub(crate) fn default_voice_test_text() -> String {
    tr!("tts_voice_test_text_default")
}

pub(crate) fn voice_test_text_too_long(field: &str) -> bool {
    field.trim().chars().count() > VOICE_TEST_TEXT_MAX_CHARS
}

pub(crate) fn spoken_voice_test_text(field: &str, default: &str) -> String {
    let text = field.trim();
    if text.is_empty() {
        return default.to_owned();
    }
    text.chars().take(VOICE_TEST_TEXT_MAX_CHARS).collect()
}

pub(crate) fn stored_voice_test_text(field: &str, default: &str) -> Option<String> {
    Some(spoken_voice_test_text(field, default)).filter(|text| text != default)
}

pub(crate) fn voice_preview_request(
    text: String,
    viewer_id: String,
    viewer_name: String,
    voice: Option<(EngineId, VoiceId)>,
) -> SpeakRequest {
    let (engine_override, voice_override) = voice.unzip();
    SpeakRequest {
        request_id: RequestId::new(),
        viewer_id,
        viewer_name,
        text,
        priority: Priority::High,
        engine_override,
        voice_override,
        source_event_id: None,
        is_reward: false,
        target: None,
        message_emotes: EmoteTokenSet::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = "Default text.";

    #[test]
    fn a_blank_field_reads_the_default_and_stores_nothing() {
        for field in ["", "   ", " \n\t "] {
            assert_eq!(spoken_voice_test_text(field, DEFAULT), DEFAULT, "{field:?}");
            assert_eq!(stored_voice_test_text(field, DEFAULT), None, "{field:?}");
        }
    }

    #[test]
    fn the_unedited_default_stays_unstored_so_it_follows_the_language() {
        assert_eq!(stored_voice_test_text(DEFAULT, DEFAULT), None);
        assert_eq!(
            stored_voice_test_text(&format!("  {DEFAULT}\n"), DEFAULT),
            None
        );
    }

    #[test]
    fn edited_text_is_stored_and_read_without_its_outer_whitespace() {
        let edited = Some("Привіт, 7 слів.".to_owned());

        assert_eq!(stored_voice_test_text(" Привіт, 7 слів. ", DEFAULT), edited);
        assert_eq!(
            Some(spoken_voice_test_text(" Привіт, 7 слів. ", DEFAULT)),
            edited
        );
    }

    #[test]
    fn the_limit_counts_characters_and_keeps_text_at_it_whole() {
        let at_limit = "є".repeat(VOICE_TEST_TEXT_MAX_CHARS);
        let past_limit = format!("{at_limit}є");

        assert!(!voice_test_text_too_long(&at_limit));
        assert!(voice_test_text_too_long(&past_limit));
        assert_eq!(spoken_voice_test_text(&at_limit, DEFAULT), at_limit);
        assert_eq!(spoken_voice_test_text(&past_limit, DEFAULT), at_limit);
        assert_eq!(stored_voice_test_text(&past_limit, DEFAULT), Some(at_limit));
    }
}
