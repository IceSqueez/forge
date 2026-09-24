use std::collections::HashSet;

use forge_components::{ToastKind, tr};
use forge_events::{Event, EventSource};
use forge_types::ClipId;
use gpui::{App, Global};
use serde_json::Value;

use crate::toasts::PushToast;

const PLAYBACK_STARTED: &str = "playback.started";
const PLAYBACK_FINISHED: &str = "playback.finished";
const PLAYBACK_FAILED: &str = "playback.failed";

#[derive(Default)]
pub struct PadPlays {
    awaiting_outcome: HashSet<ClipId>,
}

impl Global for PadPlays {}

pub trait PadPlayClaims {
    /// Claims the clip until its next playback event, so the pad's own inline error is the only surface for that attempt.
    fn claim_pad_play(&mut self, clip: ClipId);
}

impl PadPlayClaims for App {
    fn claim_pad_play(&mut self, clip: ClipId) {
        self.default_global::<PadPlays>()
            .awaiting_outcome
            .insert(clip);
    }
}

pub fn clip_id_of(payload: &Value) -> Option<ClipId> {
    let raw = payload.get("clip_id").and_then(Value::as_str)?;
    serde_json::from_value::<ClipId>(Value::String(raw.to_owned())).ok()
}

pub fn report_clip_playback(event: &Event, cx: &mut App) {
    if event.source != EventSource::Audio {
        return;
    }
    if !matches!(
        event.kind.as_str(),
        PLAYBACK_STARTED | PLAYBACK_FINISHED | PLAYBACK_FAILED
    ) {
        return;
    }
    let claimed_by_pad = clip_id_of(&event.payload).is_some_and(|clip| {
        cx.default_global::<PadPlays>()
            .awaiting_outcome
            .remove(&clip)
    });
    if event.kind != PLAYBACK_FAILED || claimed_by_pad {
        return;
    }
    cx.push_toast(ToastKind::Error, toast_text(&event.payload));
}

fn toast_text(payload: &Value) -> String {
    let error = payload
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match payload.get("clip_label").and_then(Value::as_str) {
        Some(clip) => tr!("soundboard_toast_clip_failed", clip = clip, error = error),
        None => tr!("soundboard_toast_clip_failed_unnamed", error = error),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_storage::Language;
    use gpui::TestAppContext;
    use serde_json::json;

    use super::*;
    use crate::toasts::Toasts;

    const CLIP: &str = "01J8Z3N5Q7R9T1V3X5Z7B9D1F3";

    fn clip() -> ClipId {
        CLIP.parse().unwrap()
    }

    fn setup(cx: &mut TestAppContext) {
        crate::i18n::install_language(Language::En);
        cx.update(|cx| cx.set_global(Toasts::new()));
    }

    fn audio(kind: &str, payload: Value) -> Event {
        Event::new(EventSource::Audio, kind, payload)
    }

    fn failed(label: Option<&str>) -> Event {
        audio(
            PLAYBACK_FAILED,
            json!({ "clip_id": CLIP, "clip_label": label, "error": "device lost" }),
        )
    }

    fn report(event: &Event, cx: &mut TestAppContext) {
        cx.update(|cx| report_clip_playback(event, cx));
    }

    fn claim(cx: &mut TestAppContext) {
        cx.update(|cx| cx.claim_pad_play(clip()));
    }

    fn toasts(cx: &mut TestAppContext) -> Vec<(ToastKind, String)> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|t| (t.kind, t.message.to_string()))
                .collect()
        })
    }

    #[gpui::test]
    fn a_failure_the_pad_claimed_raises_no_toast(cx: &mut TestAppContext) {
        setup(cx);
        claim(cx);

        report(&failed(Some("Air Horn")), cx);

        assert!(toasts(cx).is_empty());
    }

    #[gpui::test]
    fn a_claim_covers_only_the_first_outcome_so_the_next_failure_toasts(cx: &mut TestAppContext) {
        setup(cx);
        claim(cx);

        report(&failed(Some("Air Horn")), cx);
        report(&failed(Some("Air Horn")), cx);

        assert_eq!(toasts(cx).len(), 1);
    }

    #[gpui::test]
    fn an_unclaimed_failure_raises_one_error_toast_naming_the_clip_and_error(
        cx: &mut TestAppContext,
    ) {
        setup(cx);

        report(&failed(Some("Air Horn")), cx);

        let shown = toasts(cx);
        assert_eq!(shown.len(), 1);
        let (kind, message) = &shown[0];
        assert!(matches!(kind, ToastKind::Error));
        assert!(message.contains("Air Horn"), "{message}");
        assert!(message.contains("device lost"), "{message}");
    }

    #[gpui::test]
    fn a_start_or_finish_releases_the_claim_so_a_later_failure_toasts(cx: &mut TestAppContext) {
        setup(cx);
        for kind in [PLAYBACK_STARTED, PLAYBACK_FINISHED] {
            cx.update(|cx| cx.set_global(Toasts::new()));
            claim(cx);

            report(&audio(kind, json!({ "clip_id": CLIP })), cx);
            report(&failed(Some("Air Horn")), cx);

            assert_eq!(toasts(cx).len(), 1, "after {kind}");
        }
    }

    #[gpui::test]
    fn unrelated_events_for_the_clip_neither_toast_nor_release_the_claim(cx: &mut TestAppContext) {
        setup(cx);
        claim(cx);

        report(
            &audio("soundboard.clip.adopted", json!({ "clip_id": CLIP })),
            cx,
        );
        report(
            &Event::new(
                EventSource::Core,
                PLAYBACK_STARTED,
                json!({ "clip_id": CLIP }),
            ),
            cx,
        );
        report(
            &Event::new(
                EventSource::Core,
                PLAYBACK_FAILED,
                json!({ "clip_id": CLIP, "error": "x" }),
            ),
            cx,
        );
        report(&failed(Some("Air Horn")), cx);

        assert!(toasts(cx).is_empty());
    }

    #[gpui::test]
    fn a_failure_without_a_clip_label_still_toasts_its_error(cx: &mut TestAppContext) {
        setup(cx);

        report(&failed(None), cx);

        let shown = toasts(cx);
        assert_eq!(shown.len(), 1);
        assert!(shown[0].1.contains("device lost"), "{}", shown[0].1);
    }

    #[gpui::test]
    fn a_failure_without_a_usable_clip_id_toasts_and_leaves_claims_alone(cx: &mut TestAppContext) {
        setup(cx);
        claim(cx);

        for payload in [
            json!({ "error": "device lost" }),
            json!({ "clip_id": "not-a-ulid", "error": "device lost" }),
            json!({ "clip_id": 7, "error": "device lost" }),
        ] {
            report(&audio(PLAYBACK_FAILED, payload), cx);
        }
        report(&failed(Some("Air Horn")), cx);

        assert_eq!(toasts(cx).len(), 3);
    }
}
