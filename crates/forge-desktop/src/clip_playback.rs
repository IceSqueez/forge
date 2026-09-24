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
