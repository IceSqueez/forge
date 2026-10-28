use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_components::{
    Density, FONT_XS, ForgePalette, Icon, Radius, Spacing, body_family, fmt_clock, icon, radius,
    scaled, spacing, tr,
};
use forge_soundboard::{ClipAvailability, PlayOutcome, SoundboardError};
use forge_types::ClipId;
use gpui::{AnyElement, ClickEvent, Context, Pixels, div, prelude::*, px};

use super::{PlaybackProgress, SoundboardView};
use crate::clip_messages::failure_message;
use crate::clip_playback::PadPlayClaims;
use crate::window_presence::PresenceGate;

const TICK_INTERVAL: Duration = Duration::from_millis(100);
const STOP_ICON: Pixels = px(12.0);

impl SoundboardView {
    fn has_active_playback(&self) -> bool {
        !self.playing.is_empty()
    }

    pub(super) fn ensure_ticker(&mut self, cx: &mut Context<Self>) {
        if self.ticking || !self.has_active_playback() {
            return;
        }
        self.ticking = true;
        let mut presence = PresenceGate::of(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK_INTERVAL).await;
                presence.until_visible().await;
                let keep_going = this.update(cx, |this, cx| {
                    if this.has_active_playback() {
                        cx.notify();
                        true
                    } else {
                        this.ticking = false;
                        false
                    }
                });
                match keep_going {
                    Ok(true) => continue,
                    _ => break,
                }
            }
        })
        .detach();
    }

    pub(super) fn toggle_play(&mut self, id: ClipId, cx: &mut Context<Self>) {
        if self.playing.contains_key(&id) {
            self.player.stop(id);
            self.playing.remove(&id);
            cx.notify();
            return;
        }
        let Some(clip) = self.clips.iter().find(|c| c.id == id) else {
            return;
        };
        let known = clip.duration_secs;
        self.error = None;
        cx.claim_pad_play(id);
        self.playing.insert(
            id,
            PlaybackProgress {
                play_id: None,
                started_at: Instant::now(),
                duration_secs: known.map(f64::from),
                looped: clip.loop_playback,
            },
        );
        self.ensure_ticker(cx);
        cx.notify();

        let player_play = Arc::clone(&self.player);
        let player_dur = Arc::clone(&self.player);
        let rt = self.rt_handle.clone();
        cx.spawn(async move |this, cx| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            rt.spawn(async move {
                let result = player_play.play(id, None).await;
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
                let _ = tx.send(result);
            });
            let Ok(result) = rx.await else {
                return;
            };
            let settled_silently = play_settled_without_outcome(&result);
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| {
                if settled_silently {
                    cx.release_pad_play(id);
                }
                match &result {
                    Err(error) => this.on_play_error(id, error, cx),
                    Ok(_) if settled_silently => this.clear_ended_play(id, None, cx),
                    Ok(_) => {}
                }
            });
            if result.is_err() {
                return;
            }
            if known.is_none() {
                rt.spawn(async move {
                    if let Err(error) = player_dur.ensure_clip_duration(id).await {
                        tracing::warn!(%error, "clip duration could not be measured");
                    }
                });
            }
        })
        .detach();
    }

    fn on_play_error(&mut self, id: ClipId, error: &SoundboardError, cx: &mut Context<Self>) {
        self.playing.remove(&id);
        if let Some(availability) = availability_after_play_error(error) {
            self.availability.insert(id, availability);
        }
        let message = failure_message(error);
        self.error = Some(tr!("soundboard_playback_error_prefix", error = message.as_str()).into());
        cx.notify();
    }

    pub(super) fn clear_ended_play(
        &mut self,
        id: ClipId,
        play_id: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        if progress_tracks_play(self.playing.get(&id), play_id) {
            self.playing.remove(&id);
            cx.notify();
        }
    }

    fn stop_all(&mut self, cx: &mut Context<Self>) {
        self.player.stop_all();
        self.playing.clear();
        cx.notify();
    }

    pub(super) fn render_subheader_right(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("sb-stop-all")
            .flex()
            .items_center()
            .gap(scaled(px(5.0), density))
            .cursor_pointer()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .hover(|s| s.bg(palette.surface_overlay))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.stop_all(cx)))
            .child(icon(Icon::PlayerStop, STOP_ICON, palette.danger))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.danger)
                    .child(tr!("soundboard_stop_all")),
            )
            .into_any_element()
    }
}

fn availability_after_play_error(error: &SoundboardError) -> Option<ClipAvailability> {
    match error {
        SoundboardError::SourceMissing(_) => Some(ClipAvailability::Missing),
        _ => None,
    }
}

fn play_settled_without_outcome(result: &Result<PlayOutcome, SoundboardError>) -> bool {
    match result {
        Ok(PlayOutcome::Started) => false,
        Ok(PlayOutcome::BoardDisabled | PlayOutcome::StoppedBeforeStart) => true,
        Err(
            SoundboardError::SourceMissing(_)
            | SoundboardError::Audio(_)
            | SoundboardError::JoinError(_),
        ) => false,
        Err(
            SoundboardError::ClipNotFound(_)
            | SoundboardError::ImportRefused(_)
            | SoundboardError::Storage(_)
            | SoundboardError::MasterVolumeNotSaved(_),
        ) => true,
    }
}

fn progress_tracks_play(progress: Option<&PlaybackProgress>, play_id: Option<u64>) -> bool {
    progress.is_some_and(|progress| progress.play_id == play_id)
}

pub(super) fn readout_total(
    clip_secs: Option<f32>,
    progress: Option<&PlaybackProgress>,
) -> Option<f64> {
    match progress {
        Some(p) if p.looped => None,
        Some(p) => p.duration_secs,
        None => clip_secs.map(f64::from),
    }
    .filter(|d| *d > 0.0)
}

pub(super) fn time_readout(elapsed_secs: f64, total_secs: Option<f64>) -> String {
    match total_secs {
        Some(total) => {
            let total = total.round();
            format!(
                "{}/{}",
                fmt_clock(elapsed_secs.clamp(0.0, total) as u64),
                fmt_clock(total as u64)
            )
        }
        None => fmt_clock(elapsed_secs.max(0.0) as u64),
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::StorageError;

    use super::*;

    fn progress(duration_secs: Option<f64>, looped: bool) -> PlaybackProgress {
        PlaybackProgress {
            play_id: None,
            started_at: Instant::now(),
            duration_secs,
            looped,
        }
    }

    #[test]
    fn time_readout_floors_elapsed_and_rounds_total() {
        for (elapsed, total, expected) in [
            (0.0, 23.0, "0:00/0:23"),
            (12.9, 30.0, "0:12/0:30"),
            (59.9, 120.0, "0:59/2:00"),
            (10.0, 23.4, "0:10/0:23"),
            (10.0, 23.6, "0:10/0:24"),
        ] {
            assert_eq!(
                time_readout(elapsed, Some(total)),
                expected,
                "elapsed {elapsed} of {total}"
            );
        }
    }

    #[test]
    fn time_readout_clamps_elapsed_to_the_rounded_total() {
        for (elapsed, total, expected) in [
            (24.0, 23.0, "0:23/0:23"),
            (99.0, 23.6, "0:24/0:24"),
            (23.0, 23.0, "0:23/0:23"),
        ] {
            assert_eq!(
                time_readout(elapsed, Some(total)),
                expected,
                "elapsed {elapsed} of {total}"
            );
        }
    }

    #[test]
    fn time_readout_without_a_total_shows_elapsed_alone() {
        for (elapsed, expected) in [
            (0.0, "0:00"),
            (59.9, "0:59"),
            (60.0, "1:00"),
            (75.9, "1:15"),
        ] {
            assert_eq!(time_readout(elapsed, None), expected, "elapsed {elapsed}");
        }
    }

    #[test]
    fn readout_total_when_idle_comes_from_the_clip_duration() {
        for (clip_secs, expected) in [
            (Some(23.0_f32), Some(23.0_f64)),
            (Some(0.5), Some(0.5)),
            (Some(0.0), None),
            (None, None),
        ] {
            assert_eq!(
                readout_total(clip_secs, None),
                expected,
                "clip {clip_secs:?}"
            );
        }
    }

    #[test]
    fn readout_total_while_playing_ignores_the_clip_duration() {
        for (duration_secs, looped, expected) in [
            (Some(12.0), false, Some(12.0)),
            (None, false, None),
            (Some(30.0), true, None),
            (Some(0.0), false, None),
        ] {
            let live = progress(duration_secs, looped);
            assert_eq!(
                readout_total(Some(23.0), Some(&live)),
                expected,
                "playing {duration_secs:?} looped {looped}"
            );
        }
    }

    #[test]
    fn only_a_vanished_source_rewrites_the_row_state_after_a_failed_play() {
        for (error, expected) in [
            (
                SoundboardError::SourceMissing("Fanfare".to_owned()),
                Some(ClipAvailability::Missing),
            ),
            (SoundboardError::ClipNotFound("01J0".to_owned()), None),
            (
                SoundboardError::ImportRefused(StorageError::MediaUnsupported {
                    label: "notes.txt".to_owned(),
                }),
                None,
            ),
            (SoundboardError::Storage("disk is busy".to_owned()), None),
            (SoundboardError::JoinError("panicked".to_owned()), None),
            (
                SoundboardError::Audio(forge_audio::AudioError::NoDefaultDevice),
                None,
            ),
        ] {
            assert_eq!(availability_after_play_error(&error), expected, "{error}");
        }
    }

    #[test]
    fn only_a_play_the_player_settled_without_an_event_counts_as_outcome_free() {
        let audio = || SoundboardError::Audio(forge_audio::AudioError::NoDefaultDevice);
        for (result, expected) in [
            (Ok(PlayOutcome::BoardDisabled), true),
            (Ok(PlayOutcome::StoppedBeforeStart), false),
            (Ok(PlayOutcome::Started), false),
            (Err(SoundboardError::ClipNotFound("01J0".to_owned())), true),
            (
                Err(SoundboardError::Storage("disk is busy".to_owned())),
                true,
            ),
            (
                Err(SoundboardError::ImportRefused(
                    StorageError::MediaUnsupported {
                        label: "notes.txt".to_owned(),
                    },
                )),
                true,
            ),
            (
                Err(SoundboardError::SourceMissing("Fanfare".to_owned())),
                false,
            ),
            (Err(audio()), false),
            (
                Err(SoundboardError::JoinError("panicked".to_owned())),
                false,
            ),
        ] {
            assert_eq!(
                play_settled_without_outcome(&result),
                expected,
                "{result:?}"
            );
        }
    }
}
