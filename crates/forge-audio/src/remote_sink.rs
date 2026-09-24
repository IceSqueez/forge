use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use tokio::time::Instant;

use crate::error::AudioError;
use crate::handle::{ControlledPlayback, PlaybackHandle};
use crate::pcm::PcmBuffer;
use crate::remote::{
    ClipMediaType, RemoteAudioDestination, RemoteClip, RemoteClipId, RemoteCommand, RemoteDelivery,
    RemoteDestinationId, RemoteVerdict,
};
use crate::sink::AudioSink;
use crate::wave::encode_wave;

const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(50);
const WATCHDOG_SLACK: Duration = Duration::from_secs(45);
const WATCHDOG_CEILING: Duration = Duration::from_secs(360);
const MAX_REASON_CHARS: usize = 200;
const SINGLE_PLAYER: usize = 1;
const WATCHDOG_REASON: &str = "the page never reported whether it played the clip";

pub struct RemoteSink {
    destination: Arc<dyn RemoteAudioDestination>,
    destination_id: RemoteDestinationId,
}

impl RemoteSink {
    pub fn new(
        destination: Arc<dyn RemoteAudioDestination>,
        destination_id: RemoteDestinationId,
    ) -> Self {
        Self {
            destination,
            destination_id,
        }
    }
}

#[async_trait]
impl AudioSink for RemoteSink {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
        self.play_stoppable(buffer).await.map(|_| ())
    }

    async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        let playback = self.play_controlled(buffer).await?;
        let handle = playback.handle();
        let destination_id = self.destination_id.clone();
        tokio::spawn(async move {
            if let Err(e) = playback.await {
                tracing::warn!(
                    destination = %destination_id,
                    error = %e,
                    "remote audio clip did not play"
                );
            }
        });
        Ok(handle)
    }

    async fn play_controlled(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        let duration_ms = buffer.duration_ms();
        let clip = RemoteClip {
            bytes: encode_wave(&buffer),
            media_type: ClipMediaType::Wave,
            duration_ms,
        };

        let RemoteDelivery {
            clip_id,
            live_players,
        } = self.destination.deliver(&self.destination_id, clip).await?;

        if let Err(refusal) = admit(&self.destination_id, live_players) {
            push(
                self.destination.as_ref(),
                &self.destination_id,
                &clip_id,
                RemoteCommand::Stop,
            )
            .await;
            return Err(refusal);
        }

        if live_players > SINGLE_PLAYER {
            tracing::warn!(
                destination = %self.destination_id,
                live_players,
                "more than one audio page is live, so every one of them plays this clip"
            );
        }

        let controls = Controls::new();
        let handle =
            PlaybackHandle::from_flags(Arc::clone(&controls.stop), Arc::clone(&controls.pause));
        let budget = watchdog_budget(duration_ms);
        let clip = LiveClip {
            destination: Arc::clone(&self.destination),
            destination_id: self.destination_id.clone(),
            clip_id,
            controls,
        };

        tokio::spawn(observe_controls(clip.clone(), budget));

        let completion = settle_clip(
            StopUnlessSettled {
                clip,
                settled: false,
            },
            budget,
            live_players,
        );
        Ok(ControlledPlayback::merged(handle, Box::pin(completion)))
    }
}

#[derive(Clone)]
struct Controls {
    stop: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    stop_sent: Arc<AtomicBool>,
}

impl Controls {
    fn new() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
            finished: Arc::new(AtomicBool::new(false)),
            stop_sent: Arc::new(AtomicBool::new(false)),
        }
    }

    fn observed(&self) -> ControlState {
        ControlState {
            stopped: self.stop.load(Ordering::Relaxed),
            held: self.pause.load(Ordering::Relaxed),
        }
    }

    fn held(&self) -> bool {
        self.pause.load(Ordering::Relaxed)
    }

    fn claim_stop(&self) -> bool {
        !self.stop_sent.swap(true, Ordering::AcqRel)
    }
}

#[derive(Clone)]
struct LiveClip {
    destination: Arc<dyn RemoteAudioDestination>,
    destination_id: RemoteDestinationId,
    clip_id: RemoteClipId,
    controls: Controls,
}

impl LiveClip {
    async fn push(&self, command: RemoteCommand) {
        push(
            self.destination.as_ref(),
            &self.destination_id,
            &self.clip_id,
            command,
        )
        .await;
    }
}

/// A completion dropped before its verdict counts as a stop, and the stop still reaches the page.
struct StopUnlessSettled {
    clip: LiveClip,
    settled: bool,
}

impl Drop for StopUnlessSettled {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        let controls = &self.clip.controls;
        controls.stop.store(true, Ordering::Relaxed);
        if !controls.claim_stop() {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let clip = self.clip.clone();
        runtime.spawn(async move { clip.push(RemoteCommand::Stop).await });
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ControlState {
    stopped: bool,
    held: bool,
}

enum RemoteSettlement {
    Finished,
    Degraded(String),
    Failed(AudioError),
}

async fn observe_controls(clip: LiveClip, budget: Duration) {
    tokio::select! {
        () = run_out(&clip.controls, budget) => {}
        () = forward_commands(&clip) => {}
    }
}

async fn forward_commands(clip: &LiveClip) {
    let controls = &clip.controls;
    let mut sent = ControlState::default();

    loop {
        tokio::time::sleep(CONTROL_POLL_INTERVAL).await;

        if let Some(command) = next_command(sent, controls.observed()) {
            if command == RemoteCommand::Stop && !controls.claim_stop() {
                return;
            }
            sent = applied(sent, command);
            clip.push(command).await;
        }

        if sent.stopped
            || controls.finished.load(Ordering::Relaxed)
            || controls.stop_sent.load(Ordering::Acquire)
        {
            return;
        }
    }
}

/// Resolves once the clip has spent `budget` un-paused, or at the wall-clock ceiling while held.
async fn run_out(controls: &Controls, budget: Duration) {
    let ceiling = Instant::now() + WATCHDOG_CEILING;
    let mut remaining = budget;

    loop {
        let left_on_the_wall = ceiling.saturating_duration_since(Instant::now());
        if remaining.is_zero() || left_on_the_wall.is_zero() {
            return;
        }
        let held = controls.held();
        let step = if held {
            CONTROL_POLL_INTERVAL
        } else {
            remaining.min(CONTROL_POLL_INTERVAL)
        }
        .min(left_on_the_wall);

        tokio::time::sleep(step).await;
        if !held {
            remaining = remaining.saturating_sub(step);
        }
    }
}

async fn settle_clip(
    mut guard: StopUnlessSettled,
    budget: Duration,
    live_players: usize,
) -> Result<(), AudioError> {
    let settlement = {
        let clip = &guard.clip;
        tokio::select! {
            verdict = clip.destination.verdict(&clip.clip_id) => {
                settle(clip.controls.stop.load(Ordering::Relaxed), verdict)
            }
            () = run_out(&clip.controls, budget) => {
                RemoteSettlement::Degraded(WATCHDOG_REASON.to_owned())
            }
        }
    };
    guard.clip.controls.finished.store(true, Ordering::Relaxed);
    guard.settled = true;

    match settlement {
        RemoteSettlement::Finished => Ok(()),
        RemoteSettlement::Degraded(reason) => {
            tracing::warn!(
                destination = %guard.clip.destination_id,
                live_players,
                reason = %reason,
                "remote audio route is degraded; the clip counts as played"
            );
            Ok(())
        }
        RemoteSettlement::Failed(e) => Err(e),
    }
}

async fn push(
    destination: &dyn RemoteAudioDestination,
    destination_id: &RemoteDestinationId,
    clip_id: &RemoteClipId,
    command: RemoteCommand,
) {
    if let Err(e) = destination.control(destination_id, clip_id, command).await {
        tracing::warn!(
            destination = %destination_id,
            command = ?command,
            error = %e,
            "remote audio command did not reach the destination"
        );
    }
}

fn admit(destination_id: &RemoteDestinationId, live_players: usize) -> Result<(), AudioError> {
    if live_players == 0 {
        return Err(AudioError::NoRemoteListener {
            destination: destination_id.to_string(),
            live_players,
        });
    }
    Ok(())
}

fn next_command(sent: ControlState, observed: ControlState) -> Option<RemoteCommand> {
    if sent.stopped {
        return None;
    }
    if observed.stopped {
        return Some(RemoteCommand::Stop);
    }
    match (observed.held, sent.held) {
        (true, false) => Some(RemoteCommand::Pause),
        (false, true) => Some(RemoteCommand::Resume),
        _ => None,
    }
}

fn applied(sent: ControlState, command: RemoteCommand) -> ControlState {
    match command {
        RemoteCommand::Stop => ControlState {
            stopped: true,
            ..sent
        },
        RemoteCommand::Pause => ControlState { held: true, ..sent },
        RemoteCommand::Resume => ControlState {
            held: false,
            ..sent
        },
    }
}

fn watchdog_budget(duration_ms: u64) -> Duration {
    Duration::from_millis(duration_ms)
        .saturating_add(WATCHDOG_SLACK)
        .min(WATCHDOG_CEILING)
}

fn settle(stopped: bool, verdict: Result<RemoteVerdict, AudioError>) -> RemoteSettlement {
    if stopped {
        return RemoteSettlement::Finished;
    }
    match verdict {
        Ok(RemoteVerdict::Played) => RemoteSettlement::Finished,
        Ok(RemoteVerdict::Unknown { reason }) => {
            RemoteSettlement::Degraded(bounded_reason(&reason))
        }
        Ok(RemoteVerdict::Refused { reason }) => {
            RemoteSettlement::Failed(AudioError::RemoteDestination(bounded_reason(&reason)))
        }
        Err(e) => RemoteSettlement::Failed(e),
    }
}

fn bounded_reason(reason: &str) -> String {
    reason
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_REASON_CHARS)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    use tokio::sync::{Notify, mpsc};

    use super::*;
    use crate::wave::tests::parse_header;

    const DESTINATION: &str = "audio-overlay";
    const CLIP: &str = "clip-cap-3Kd8QmPr";
    const PAGE_REFUSAL: &str = "the browser rejected the play attempt";
    const TRANSPORT_FAULT: &str = "the overlay channel closed mid-clip";
    const CLOCK_RATE_HZ: u32 = 1_000;
    const QUIET: Duration = Duration::from_secs(5);

    enum Report {
        Now(RemoteVerdict),
        Never,
        OnStop(RemoteVerdict),
        WhenTold(RemoteVerdict),
        Fails(String),
    }

    struct FakePage {
        live_players: usize,
        report: Report,
        commands: mpsc::UnboundedSender<RemoteCommand>,
        delivered: Mutex<Vec<(Vec<u8>, u64)>>,
        verdict_calls: AtomicUsize,
        stopped: Notify,
        told: Notify,
    }

    impl FakePage {
        fn new(
            live_players: usize,
            report: Report,
        ) -> (Arc<Self>, mpsc::UnboundedReceiver<RemoteCommand>) {
            let (commands, rx) = mpsc::unbounded_channel();
            let page = Arc::new(Self {
                live_players,
                report,
                commands,
                delivered: Mutex::new(Vec::new()),
                verdict_calls: AtomicUsize::new(0),
                stopped: Notify::new(),
                told: Notify::new(),
            });
            (page, rx)
        }

        fn verdict_calls(&self) -> usize {
            self.verdict_calls.load(Ordering::Relaxed)
        }

        fn first_delivery(&self) -> Option<(Vec<u8>, u64)> {
            self.delivered.lock().unwrap().first().cloned()
        }
    }

    #[async_trait]
    impl RemoteAudioDestination for FakePage {
        async fn deliver(
            &self,
            _destination: &RemoteDestinationId,
            clip: RemoteClip,
        ) -> Result<RemoteDelivery, AudioError> {
            self.delivered
                .lock()
                .unwrap()
                .push((clip.bytes, clip.duration_ms));
            Ok(RemoteDelivery {
                clip_id: RemoteClipId::new(CLIP),
                live_players: self.live_players,
            })
        }

        async fn control(
            &self,
            _destination: &RemoteDestinationId,
            _clip_id: &RemoteClipId,
            command: RemoteCommand,
        ) -> Result<(), AudioError> {
            let _ = self.commands.send(command);
            if command == RemoteCommand::Stop {
                self.stopped.notify_one();
            }
            Ok(())
        }

        async fn verdict(&self, _clip_id: &RemoteClipId) -> Result<RemoteVerdict, AudioError> {
            self.verdict_calls.fetch_add(1, Ordering::Relaxed);
            match &self.report {
                Report::Now(verdict) => Ok(verdict.clone()),
                Report::Never => std::future::pending().await,
                Report::OnStop(verdict) => {
                    self.stopped.notified().await;
                    Ok(verdict.clone())
                }
                Report::WhenTold(verdict) => {
                    self.told.notified().await;
                    Ok(verdict.clone())
                }
                Report::Fails(reason) => Err(AudioError::RemoteDestination(reason.clone())),
            }
        }
    }

    fn sink(page: &Arc<FakePage>) -> RemoteSink {
        let destination: Arc<dyn RemoteAudioDestination> = page.clone();
        RemoteSink::new(destination, RemoteDestinationId::new(DESTINATION))
    }

    fn clip_of(duration_ms: u64) -> PcmBuffer {
        PcmBuffer::new(vec![0; duration_ms as usize], CLOCK_RATE_HZ, 1)
    }

    async fn finish(sink: RemoteSink, buffer: PcmBuffer) -> Result<(), AudioError> {
        sink.play_controlled(buffer).await?.await
    }

    async fn next_pushed(rx: &mut mpsc::UnboundedReceiver<RemoteCommand>) -> Option<RemoteCommand> {
        tokio::time::timeout(QUIET, rx.recv()).await.ok().flatten()
    }

    #[tokio::test(start_paused = true)]
    async fn a_route_with_no_live_page_fails_at_send_time_without_waiting_for_a_verdict() {
        let (page, _rx) = FakePage::new(0, Report::Never);

        let refusal = sink(&page).play_controlled(clip_of(20)).await.err();

        assert!(
            matches!(
                &refusal,
                Some(AudioError::NoRemoteListener { destination, live_players })
                    if destination == DESTINATION && *live_players == 0
            ),
            "a route nobody listens to must name the overlay and the count, got {refusal:?}"
        );
        assert_eq!(
            page.verdict_calls(),
            0,
            "a send that reached nobody must not wait on a page that is not there"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn every_live_player_count_above_zero_is_admitted_and_plays_the_clip() {
        for live_players in [1_usize, 2, 7] {
            let (page, _rx) = FakePage::new(live_players, Report::Now(RemoteVerdict::Played));

            let outcome = finish(sink(&page), clip_of(20)).await;

            assert!(
                outcome.is_ok(),
                "{live_players} live pages all play the clip, got {outcome:?}"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_refusal_fails_the_clip_and_carries_the_reason_the_page_gave() {
        let (page, _rx) = FakePage::new(
            1,
            Report::Now(RemoteVerdict::Refused {
                reason: PAGE_REFUSAL.to_owned(),
            }),
        );

        let outcome = finish(sink(&page), clip_of(20)).await;

        assert!(
            matches!(&outcome, Err(AudioError::RemoteDestination(reason)) if reason == PAGE_REFUSAL),
            "a page that refused the clip must fail the utterance with its own words, got {outcome:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_hostile_refusal_reason_is_stripped_and_capped_before_it_reaches_the_error() {
        for (raw, expected) in [
            ("a\nb\tc\u{7}d".to_owned(), "abcd".to_owned()),
            ("x".repeat(MAX_REASON_CHARS), "x".repeat(MAX_REASON_CHARS)),
            (
                "y".repeat(MAX_REASON_CHARS + 1),
                "y".repeat(MAX_REASON_CHARS),
            ),
            (
                "ф".repeat(MAX_REASON_CHARS + 1),
                "ф".repeat(MAX_REASON_CHARS),
            ),
        ] {
            let (page, _rx) = FakePage::new(
                1,
                Report::Now(RemoteVerdict::Refused {
                    reason: raw.clone(),
                }),
            );

            let outcome = finish(sink(&page), clip_of(20)).await;

            let Some(AudioError::RemoteDestination(reason)) = outcome.err() else {
                panic!(
                    "a refusal of {} chars must fail the clip",
                    raw.chars().count()
                );
            };
            assert_eq!(
                reason,
                expected,
                "a {}-char reason reached the error unbounded",
                raw.chars().count()
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn an_unknown_verdict_finishes_the_clip_instead_of_failing_the_utterance() {
        let (page, _rx) = FakePage::new(
            1,
            Report::Now(RemoteVerdict::Unknown {
                reason: "the page never said whether it played".to_owned(),
            }),
        );

        let outcome = finish(sink(&page), clip_of(20)).await;

        assert!(
            outcome.is_ok(),
            "a destination that cannot tell must degrade, not fail a clip the stream probably heard, got {outcome:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_silent_destination_settles_at_the_clip_duration_plus_slack_capped_at_the_ceiling() {
        for (duration_ms, expected) in [
            (0_u64, WATCHDOG_SLACK),
            (4_000, WATCHDOG_SLACK + Duration::from_secs(4)),
            (400_000, WATCHDOG_CEILING),
        ] {
            let (page, _rx) = FakePage::new(1, Report::Never);
            let started = tokio::time::Instant::now();

            let outcome = finish(sink(&page), clip_of(duration_ms)).await;
            let waited = started.elapsed();

            assert!(
                outcome.is_ok(),
                "a page that never reports must not fail a {duration_ms} ms clip, got {outcome:?}"
            );
            assert!(
                waited >= expected && waited < expected + CONTROL_POLL_INTERVAL,
                "a {duration_ms} ms clip must settle at {expected:?}, waited {waited:?}"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn stopping_a_clip_pushes_one_stop_and_finishes_cleanly_long_before_the_watchdog() {
        let (page, mut rx) = FakePage::new(
            1,
            Report::OnStop(RemoteVerdict::Refused {
                reason: PAGE_REFUSAL.to_owned(),
            }),
        );
        let playback = sink(&page).play_controlled(clip_of(4_000)).await.unwrap();

        playback.stop();
        let started = tokio::time::Instant::now();
        let outcome = playback.await;
        let waited = started.elapsed();

        assert!(
            outcome.is_ok(),
            "a clip the caller stopped finishes cleanly however the page reports it, got {outcome:?}"
        );
        assert!(
            waited <= CONTROL_POLL_INTERVAL * 2,
            "a stop must release the queue at once, not wait the watchdog out, waited {waited:?}"
        );
        assert_eq!(next_pushed(&mut rx).await, Some(RemoteCommand::Stop));
        assert_eq!(
            next_pushed(&mut rx).await,
            None,
            "the stop must reach the page exactly once"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn pausing_and_resuming_reach_the_page_as_an_ordered_pair() {
        let (page, mut rx) = FakePage::new(1, Report::Never);
        let playback = sink(&page).play_controlled(clip_of(4_000)).await.unwrap();

        playback.pause();
        let held = next_pushed(&mut rx).await;
        playback.resume();
        let released = next_pushed(&mut rx).await;

        assert_eq!(
            held,
            Some(RemoteCommand::Pause),
            "a paused clip must be held on the page"
        );
        assert_eq!(
            released,
            Some(RemoteCommand::Resume),
            "a resumed clip must start again on the page"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn the_control_observer_retires_with_the_clip_so_a_late_stop_reaches_nobody() {
        let (page, mut rx) = FakePage::new(1, Report::Now(RemoteVerdict::Played));
        let playback = sink(&page).play_controlled(clip_of(4_000)).await.unwrap();
        let handle = playback.handle();

        assert!(playback.await.is_ok());
        assert_eq!(
            next_pushed(&mut rx).await,
            None,
            "a clip the page played needs no command"
        );

        handle.stop();

        assert_eq!(
            next_pushed(&mut rx).await,
            None,
            "the observer must retire with the clip instead of outliving it"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn the_clip_reaches_the_page_at_the_rate_and_channel_count_the_engine_produced() {
        let (page, _rx) = FakePage::new(1, Report::Now(RemoteVerdict::Played));
        let buffer = PcmBuffer::new(vec![9; 96], 48_000, 2);
        let (rate, channels, duration) =
            (buffer.sample_rate, buffer.channels, buffer.duration_ms());

        finish(sink(&page), buffer).await.unwrap();

        let (bytes, announced_ms) = page.first_delivery().unwrap();
        let header = parse_header(&bytes);
        assert_eq!(
            header.sample_rate, rate,
            "the remote leg must not resample what the local route hears"
        );
        assert_eq!(
            header.channels, channels,
            "the remote leg must not remix what the local route hears"
        );
        assert_eq!(
            announced_ms, duration,
            "the page must be told the clip's real length"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_destination_that_cannot_be_asked_at_all_fails_the_clip() {
        let (page, _rx) = FakePage::new(1, Report::Fails(TRANSPORT_FAULT.to_owned()));

        let outcome = finish(sink(&page), clip_of(20)).await;

        assert!(
            matches!(&outcome, Err(AudioError::RemoteDestination(reason)) if reason == TRANSPORT_FAULT),
            "a transport that broke mid-clip must reach the caller, not count as played, got {outcome:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_fire_and_forget_play_still_hands_back_a_handle_that_stops_the_page() {
        let (page, mut rx) = FakePage::new(
            1,
            Report::OnStop(RemoteVerdict::Refused {
                reason: PAGE_REFUSAL.to_owned(),
            }),
        );

        let handle = sink(&page).play_stoppable(clip_of(4_000)).await.unwrap();
        handle.stop();

        assert_eq!(
            next_pushed(&mut rx).await,
            Some(RemoteCommand::Stop),
            "a clip started without a completion future must still be stoppable"
        );
    }

    async fn assert_one_stop_then_quiet(
        case: &str,
        rx: &mut mpsc::UnboundedReceiver<RemoteCommand>,
    ) {
        assert_eq!(
            next_pushed(rx).await,
            Some(RemoteCommand::Stop),
            "{case}: the page must be told to stop"
        );
        assert_eq!(
            next_pushed(rx).await,
            None,
            "{case}: the stop must reach the page exactly once"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_an_unsettled_playback_stops_the_clip_on_the_page_exactly_once() {
        for (case, stop_first, poll_first) in [
            ("dropped before it was ever awaited", false, false),
            ("dropped while it waited on the verdict", false, true),
            ("stopped and dropped in the same step", true, true),
        ] {
            let (page, mut rx) = FakePage::new(1, Report::Never);
            let mut playback = sink(&page).play_controlled(clip_of(4_000)).await.unwrap();
            if poll_first {
                let parked = tokio::time::timeout(CONTROL_POLL_INTERVAL / 2, &mut playback).await;
                assert!(parked.is_err(), "{case}: the clip settled on its own");
            }
            if stop_first {
                playback.stop();
            }

            drop(playback);

            assert_one_stop_then_quiet(case, &mut rx).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_a_fan_out_playback_stops_its_remote_leg_exactly_once() {
        let (page, mut rx) = FakePage::new(1, Report::Never);
        let fan_out = crate::fan_out::FanOutSink::new(vec![
            Arc::new(sink(&page)) as Arc<dyn AudioSink>,
            Arc::new(crate::sink::NullSink),
        ]);
        let playback = fan_out.play_controlled(clip_of(4_000)).await.unwrap();

        drop(playback);

        assert_one_stop_then_quiet("fan-out leg", &mut rx).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_pause_longer_than_the_watchdog_budget_still_settles_on_the_page_verdict() {
        let (page, _rx) = FakePage::new(
            1,
            Report::WhenTold(RemoteVerdict::Refused {
                reason: PAGE_REFUSAL.to_owned(),
            }),
        );
        let playback = sink(&page).play_controlled(clip_of(20)).await.unwrap();
        let handle = playback.handle();
        handle.pause();
        let settling = tokio::spawn(playback);

        tokio::time::sleep(watchdog_budget(20) * 2).await;
        handle.resume();
        page.told.notify_one();
        let outcome = settling.await.unwrap();

        assert!(
            matches!(&outcome, Err(AudioError::RemoteDestination(reason)) if reason == PAGE_REFUSAL),
            "paused time must not count toward the watchdog, got {outcome:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_resume_after_a_pause_longer_than_the_watchdog_budget_still_reaches_the_page() {
        let (page, mut rx) = FakePage::new(1, Report::Never);
        let playback = sink(&page).play_controlled(clip_of(20)).await.unwrap();

        playback.pause();
        assert_eq!(next_pushed(&mut rx).await, Some(RemoteCommand::Pause));
        tokio::time::sleep(watchdog_budget(20) * 2).await;
        playback.resume();

        assert_eq!(
            next_pushed(&mut rx).await,
            Some(RemoteCommand::Resume),
            "the observer must still forward controls after a long pause"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_clip_held_forever_still_settles_at_the_wall_clock_ceiling() {
        let (page, _rx) = FakePage::new(1, Report::Never);
        let playback = sink(&page).play_controlled(clip_of(20)).await.unwrap();
        playback.pause();
        let started = tokio::time::Instant::now();

        let Ok(outcome) = tokio::time::timeout(WATCHDOG_CEILING * 2, playback).await else {
            panic!("a held clip must not wait forever");
        };
        let waited = started.elapsed();

        assert!(
            outcome.is_ok(),
            "a clip held to the ceiling degrades, got {outcome:?}"
        );
        assert!(
            waited >= WATCHDOG_CEILING && waited < WATCHDOG_CEILING + CONTROL_POLL_INTERVAL,
            "a held clip must settle at the ceiling, waited {waited:?}"
        );
    }
}
