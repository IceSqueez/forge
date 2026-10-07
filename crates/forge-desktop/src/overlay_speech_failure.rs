use forge_components::{ToastKind, tr};
use forge_events::{Event, EventSource};
use forge_runtime::{OVERLAY_NAME_KEY, OVERLAY_SPEECH_FAILED_KIND, SPEECH_FAILURE_ERROR_KEY};
use gpui::App;
use serde_json::Value;

use crate::toasts::PushToast;

pub fn report_overlay_speech_failure(event: &Event, cx: &mut App) {
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
    Some(tr!(
        "overlays_toast_speech_failed",
        overlay = text_of(OVERLAY_NAME_KEY),
        error = text_of(SPEECH_FAILURE_ERROR_KEY)
    ))
}

#[cfg(test)]
mod tests {
    use forge_runtime::OVERLAY_SPEECH_FAILED_KIND;

    use super::*;

    const UNINSTALLED: &str = "voice \"amy\" of engine \"piper\" is not installed";

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

    #[test]
    fn a_speech_failure_toast_names_the_overlay_and_the_reason() {
        crate::i18n::install_language(forge_storage::Language::En);

        let text = overlay_speech_failure_text(&failure_from(
            EventSource::Core,
            OVERLAY_SPEECH_FAILED_KIND,
        ))
        .map(|text| text.replace(['\u{2068}', '\u{2069}'], ""));

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
