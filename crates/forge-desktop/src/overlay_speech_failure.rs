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
