use super::play_state::{PlayControl, PlayReporter, Reservation};
use super::{PLAYBACK_TAIL_MS, PlayOutcome};
use std::sync::Arc;
use std::time::Duration;

use forge_audio::{AudioError, AudioSink, ControlledPlayback, PlaybackHandle};
use forge_speech_types::PcmBuffer;
use tokio::sync::oneshot;

use crate::error::SoundboardError;

pub(super) struct StartedLegs {
    pub(super) handles: Vec<PlaybackHandle>,
    pub(super) main: ControlledPlayback,
}

pub(super) fn reported_failure(
    reporter: &PlayReporter,
    play: &Reservation,
    error: SoundboardError,
) -> Result<PlayOutcome, SoundboardError> {
    if play.control.is_stopped() {
        return Ok(PlayOutcome::StoppedBeforeStart);
    }
    let reason = match &error {
        SoundboardError::Audio(audio) => audio.to_string(),
        other => other.to_string(),
    };
    reporter.fail(play, reason);
    Err(error)
}

pub(super) fn watch_main_leg(main: ControlledPlayback) -> oneshot::Receiver<AudioError> {
    let (failure_tx, failure_rx) = oneshot::channel();
    tokio::spawn(async move {
        if let Err(e) = main.await {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the player may stop watching once playback ends"
            )]
            let _ = failure_tx.send(e);
        }
    });
    failure_rx
}

pub(super) async fn main_leg_failure(failure: oneshot::Receiver<AudioError>) -> AudioError {
    match failure.await {
        Ok(error) => error,
        Err(_) => std::future::pending().await,
    }
}

pub(super) async fn watch_single(
    play: Reservation,
    reporter: PlayReporter,
    main: ControlledPlayback,
    duration_ms: u64,
) {
    tokio::select! {
        error = main_leg_failure(watch_main_leg(main)) => reporter.fail(&play, error.to_string()),
        () = tokio::time::sleep(Duration::from_millis(duration_ms + PLAYBACK_TAIL_MS)) => {
            reporter.finish(&play);
        }
        () = play.control.stopped() => {}
    }
}

pub(super) struct LoopPlay {
    pub(super) play: Reservation,
    pub(super) reporter: PlayReporter,
    pub(super) sinks: Vec<Arc<dyn AudioSink>>,
    pub(super) buffer: PcmBuffer,
    pub(super) cycle_ms: u64,
    pub(super) main: ControlledPlayback,
}

pub(super) async fn run_loop(looped: LoopPlay) {
    let LoopPlay {
        play,
        reporter,
        sinks,
        buffer,
        cycle_ms,
        main,
    } = looped;
    let mut failure = watch_main_leg(main);
    loop {
        tokio::select! {
            error = main_leg_failure(failure) => {
                reporter.fail(&play, error.to_string());
                return;
            }
            () = wait_cycle(&play.control, cycle_ms) => {}
        }
        if play.control.is_stopped() {
            break;
        }
        match play_target(&sinks, buffer.clone()).await {
            Ok(legs) => {
                if !play.control.install(legs.handles) {
                    break;
                }
                failure = watch_main_leg(legs.main);
            }
            Err(e) => {
                reporter.fail(&play, e.to_string());
                return;
            }
        }
    }
    reporter.finish(&play);
}

pub(super) async fn wait_cycle(control: &PlayControl, cycle_ms: u64) {
    tokio::select! {
        () = control.stopped() => {}
        () = tokio::time::sleep(Duration::from_millis(cycle_ms)) => {}
    }
}

pub(super) async fn play_target(
    sinks: &[Arc<dyn AudioSink>],
    buffer: PcmBuffer,
) -> Result<StartedLegs, AudioError> {
    let Some((last_sink, other_sinks)) = sinks.split_last() else {
        return Err(AudioError::NoRoute);
    };
    let other_starts = other_sinks.iter().map(|sink| {
        let sink = Arc::clone(sink);
        let buffer = buffer.clone();
        async move { sink.play_controlled(buffer).await }
    });
    let (mut outcomes, last_outcome) = tokio::join!(
        futures::future::join_all(other_starts),
        last_sink.play_controlled(buffer),
    );
    outcomes.push(last_outcome);
    let mut outcomes = outcomes.into_iter();
    let Some(main) = outcomes.next() else {
        return Err(AudioError::NoRoute);
    };

    let mut secondary = Vec::new();
    for (idx, outcome) in outcomes.enumerate() {
        match outcome {
            Ok(leg) => {
                secondary.push(leg.handle());
                tokio::spawn(watch_secondary(idx + 1, leg));
            }
            Err(e) => {
                tracing::warn!(sink_index = idx + 1, error = %e, "secondary output sink failed");
            }
        }
    }

    match main {
        Ok(main) => {
            let mut handles = vec![main.handle()];
            handles.extend(secondary);
            Ok(StartedLegs { handles, main })
        }
        Err(e) => {
            for handle in &secondary {
                handle.stop();
            }
            Err(e)
        }
    }
}

pub(super) async fn watch_secondary(sink_index: usize, leg: ControlledPlayback) {
    if let Err(e) = leg.await {
        tracing::warn!(sink_index, error = %e, "secondary output sink stopped during playback");
    }
}
