use std::collections::HashMap;
use std::time::{Duration, Instant};

use forge_components::{ToastKind, tr};
use forge_soundboard::vocabulary::PlaybackFailed;
use forge_types::ClipId;
use gpui::{App, Global};

use forge_components::PushToast;

use crate::runtime_event::ClipPlayback;

const PAD_CLAIM_LIFETIME: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct PadPlays {
    awaiting_outcome: HashMap<ClipId, Instant>,
}

impl PadPlays {
    fn claim(&mut self, clip: ClipId) {
        self.awaiting_outcome
            .retain(|_, claimed_at| claimed_at.elapsed() < PAD_CLAIM_LIFETIME);
        self.awaiting_outcome.insert(clip, Instant::now());
    }

    fn take_live_claim(&mut self, clip: &ClipId) -> bool {
        self.awaiting_outcome
            .remove(clip)
            .is_some_and(|claimed_at| claimed_at.elapsed() < PAD_CLAIM_LIFETIME)
    }
}

impl Global for PadPlays {}

pub trait PadPlayClaims {
    fn claim_pad_play(&mut self, clip: ClipId);
    fn release_pad_play(&mut self, clip: ClipId);
}

impl PadPlayClaims for App {
    fn claim_pad_play(&mut self, clip: ClipId) {
        self.default_global::<PadPlays>().claim(clip);
    }

    fn release_pad_play(&mut self, clip: ClipId) {
        self.default_global::<PadPlays>()
            .awaiting_outcome
            .remove(&clip);
    }
}

pub fn report_clip_playback(playback: &ClipPlayback, cx: &mut App) {
    let claimed_by_pad = playback
        .clip_id()
        .is_some_and(|clip| cx.default_global::<PadPlays>().take_live_claim(&clip));
    let ClipPlayback::Failed(failed) = playback else {
        return;
    };
    if claimed_by_pad {
        return;
    }
    cx.push_toast(ToastKind::Error, toast_text(failed));
}

fn toast_text(failed: &PlaybackFailed) -> String {
    match failed.clip_label.as_deref() {
        Some(clip) => tr!(
            "soundboard_toast_clip_failed",
            clip = clip,
            error = failed.error.as_str()
        ),
        None => tr!(
            "soundboard_toast_clip_failed_unnamed",
            error = failed.error.as_str()
        ),
    }
}

#[cfg(test)]
mod tests {
    use forge_soundboard::vocabulary::{PlaybackFinished, PlaybackStarted};
    use forge_storage::Language;
    use gpui::TestAppContext;

    use super::*;
    use forge_components::Toasts;

    const CLIP: &str = "01J8Z3N5Q7R9T1V3X5Z7B9D1F3";

    fn clip() -> ClipId {
        CLIP.parse().unwrap()
    }

    fn setup(cx: &mut TestAppContext) {
        crate::i18n::install_language(Language::En);
        cx.update(|cx| cx.set_global(Toasts::new()));
    }

    fn failed_for(clip_id: Option<ClipId>, label: Option<&str>) -> ClipPlayback {
        ClipPlayback::Failed(PlaybackFailed {
            play_id: 1,
            clip_id,
            clip_label: label.map(str::to_owned),
            error: "device lost".to_owned(),
        })
    }

    fn failed(label: Option<&str>) -> ClipPlayback {
        failed_for(Some(clip()), label)
    }

    fn started() -> ClipPlayback {
        ClipPlayback::Started(PlaybackStarted {
            play_id: 1,
            clip_id: Some(clip()),
            clip_label: None,
            device: "default".to_owned(),
            duration_secs: None,
            looped: false,
        })
    }

    fn finished() -> ClipPlayback {
        ClipPlayback::Finished(PlaybackFinished {
            play_id: 1,
            clip_id: Some(clip()),
            clip_label: None,
        })
    }

    fn report(playback: &ClipPlayback, cx: &mut TestAppContext) {
        cx.update(|cx| report_clip_playback(playback, cx));
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
        for outcome in [started(), finished()] {
            cx.update(|cx| cx.set_global(Toasts::new()));
            claim(cx);

            report(&outcome, cx);
            report(&failed(Some("Air Horn")), cx);

            assert_eq!(toasts(cx).len(), 1, "after {outcome:?}");
        }
    }

    #[gpui::test]
    fn a_start_or_finish_never_toasts(cx: &mut TestAppContext) {
        setup(cx);

        report(&started(), cx);
        report(&finished(), cx);

        assert!(toasts(cx).is_empty());
    }

    #[gpui::test]
    fn a_released_claim_lets_the_next_failure_toast(cx: &mut TestAppContext) {
        setup(cx);
        claim(cx);

        cx.update(|cx| cx.release_pad_play(clip()));
        report(&failed(Some("Air Horn")), cx);

        assert_eq!(toasts(cx).len(), 1);
    }

    #[gpui::test]
    fn releasing_a_clip_that_holds_no_claim_leaves_other_claims_in_place(cx: &mut TestAppContext) {
        setup(cx);
        claim(cx);

        cx.update(|cx| {
            cx.release_pad_play(ClipId::new());
            cx.release_pad_play(ClipId::new());
        });
        report(&failed(Some("Air Horn")), cx);

        assert!(toasts(cx).is_empty());
    }

    #[gpui::test]
    fn releasing_twice_after_a_claim_still_toasts_the_next_failure_once(cx: &mut TestAppContext) {
        setup(cx);
        claim(cx);

        cx.update(|cx| {
            cx.release_pad_play(clip());
            cx.release_pad_play(clip());
        });
        report(&failed(Some("Air Horn")), cx);

        assert_eq!(toasts(cx).len(), 1);
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
    fn failures_of_other_or_unknown_clips_toast_and_leave_the_claim_alone(cx: &mut TestAppContext) {
        setup(cx);
        claim(cx);

        report(&failed_for(None, None), cx);
        report(&failed_for(Some(ClipId::new()), Some("Other")), cx);
        report(&failed(Some("Air Horn")), cx);

        assert_eq!(toasts(cx).len(), 2);
    }

    fn claim_aged_past_its_lifetime(cx: &mut TestAppContext) -> bool {
        let Some(claimed_at) = Instant::now().checked_sub(PAD_CLAIM_LIFETIME) else {
            return false;
        };
        cx.update(|cx| {
            cx.default_global::<PadPlays>()
                .awaiting_outcome
                .insert(clip(), claimed_at);
        });
        true
    }

    #[gpui::test]
    fn a_failure_after_the_claim_expired_is_toasted(cx: &mut TestAppContext) {
        setup(cx);
        if !claim_aged_past_its_lifetime(cx) {
            return;
        }

        report(&failed(Some("airhorn")), cx);

        assert_eq!(toasts(cx).len(), 1);
    }

    #[gpui::test]
    fn claiming_another_clip_drops_claims_whose_outcome_never_came(cx: &mut TestAppContext) {
        setup(cx);
        if !claim_aged_past_its_lifetime(cx) {
            return;
        }
        let other: ClipId = "01J8Z3N5Q7R9T1V3X5Z7B9D1F4".parse().unwrap();

        cx.update(|cx| cx.claim_pad_play(other));

        let held = cx.update(|cx| {
            cx.default_global::<PadPlays>()
                .awaiting_outcome
                .keys()
                .copied()
                .collect::<Vec<_>>()
        });
        assert_eq!(held, [other]);
    }
}
