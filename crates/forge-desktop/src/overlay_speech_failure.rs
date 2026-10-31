use forge_components::{ToastKind, tr};
use forge_events::{Event, EventSource};
use forge_runtime::{
    OVERLAY_NAME_KEY, OVERLAY_SPEECH_FAILED_KIND, SPEECH_FAILURE_ERROR_KEY,
    SPEECH_FAILURE_MESSAGE_KEY,
};
use forge_types::MessageRef;
use gpui::App;
use serde::Deserialize;
use serde_json::Value;

use forge_components::PushToast;

use crate::user_text::resolve_message;

pub(crate) fn report_overlay_speech_failure(event: &Event, cx: &mut App) {
    if let Some(message) = overlay_speech_failure_text(event) {
        cx.push_toast(ToastKind::Error, message);
    }
}

fn overlay_speech_failure_text(event: &Event) -> Option<String> {
    if event.source != EventSource::Core || event.kind != OVERLAY_SPEECH_FAILED_KIND {
        return None;
    }
    let text_of = |key: &str| {
        event
            .payload
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    let reason = event
        .payload
        .get(SPEECH_FAILURE_MESSAGE_KEY)
        .and_then(|message| MessageRef::deserialize(message).ok())
        .map(|message| resolve_message(&message).inline())
        .unwrap_or_else(|| text_of(SPEECH_FAILURE_ERROR_KEY).to_owned());
    Some(tr!(
        "overlays_toast_speech_failed",
        overlay = text_of(OVERLAY_NAME_KEY),
        error = reason
    ))
}

#[cfg(test)]
mod tests {
    use forge_runtime::OVERLAY_SPEECH_FAILED_KIND;
    use forge_types::{MessageCode, VerbatimText};

    use super::*;

    const UNINSTALLED: &str = "core.voice_not_installed";

    fn failure_from(source: EventSource, kind: &str) -> Event {
        Event::new(
            source,
            kind,
            serde_json::json!({
                OVERLAY_NAME_KEY: "Stage alert",
                SPEECH_FAILURE_ERROR_KEY: UNINSTALLED,
            }),
        )
    }

    fn failure_with_message() -> Event {
        Event::new(
            EventSource::Core,
            OVERLAY_SPEECH_FAILED_KIND,
            serde_json::json!({
                OVERLAY_NAME_KEY: "Stage alert",
                SPEECH_FAILURE_ERROR_KEY: UNINSTALLED,
                SPEECH_FAILURE_MESSAGE_KEY: MessageRef::new(MessageCode::from_static(
                    "core",
                    "voice_not_installed"
                ))
                .with_arg("voice", VerbatimText::new("piper/amy")),
            }),
        )
    }

    fn plain(text: Option<String>) -> String {
        text.expect("a speech failure becomes a toast")
            .replace(crate::test_support::FORMAT_ISOLATES, "")
    }

    #[test]
    fn a_failure_carrying_its_message_is_shown_in_the_active_language() {
        crate::i18n::install_language(forge_storage::Language::En);
        let english = plain(overlay_speech_failure_text(&failure_with_message()));
        crate::i18n::install_language(forge_storage::Language::Uk);
        let ukrainian = plain(overlay_speech_failure_text(&failure_with_message()));
        crate::i18n::install_language(forge_storage::Language::En);

        assert!(
            english.contains("The voice piper/amy is not installed"),
            "{english}"
        );
        assert!(
            ukrainian.contains("Голос piper/amy не встановлено"),
            "{ukrainian}"
        );
        assert!(!english.contains(UNINSTALLED) && !ukrainian.contains(UNINSTALLED));
    }

    #[test]
    fn a_failure_without_a_message_falls_back_to_the_plain_error_text() {
        crate::i18n::install_language(forge_storage::Language::En);
        let text = plain(overlay_speech_failure_text(&failure_from(
            EventSource::Core,
            OVERLAY_SPEECH_FAILED_KIND,
        )));

        assert!(text.contains(UNINSTALLED), "{text}");
    }

    #[test]
    fn a_malformed_message_falls_back_to_the_plain_error_text() {
        crate::i18n::install_language(forge_storage::Language::En);
        let mut event = failure_with_message();
        event.payload[SPEECH_FAILURE_MESSAGE_KEY] = serde_json::json!({"code": 7});

        let text = plain(overlay_speech_failure_text(&event));

        assert!(text.contains(UNINSTALLED), "{text}");
    }

    #[test]
    fn a_speech_failure_toast_names_the_overlay_and_the_reason() {
        crate::i18n::install_language(forge_storage::Language::En);

        let text = overlay_speech_failure_text(&failure_from(
            EventSource::Core,
            OVERLAY_SPEECH_FAILED_KIND,
        ))
        .map(|text| text.replace(crate::test_support::FORMAT_ISOLATES, ""));

        assert!(
            text.as_deref()
                .is_some_and(|text| text.starts_with("Stage alert ") && text.contains(UNINSTALLED)),
            "{text:?}"
        );
    }

    #[test]
    fn only_a_core_overlay_speech_failure_becomes_a_toast() {
        for (source, kind) in [
            (EventSource::Core, "overlay.test_fire"),
            (EventSource::Core, "playback.failed"),
            (EventSource::Rhai, OVERLAY_SPEECH_FAILED_KIND),
            (EventSource::Http, OVERLAY_SPEECH_FAILED_KIND),
        ] {
            let text = overlay_speech_failure_text(&failure_from(source, kind));

            assert_eq!(text, None, "{source:?} {kind}");
        }
    }
}
