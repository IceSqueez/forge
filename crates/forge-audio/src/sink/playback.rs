use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};
use std::thread::Thread;

use tokio::sync::Notify;

use crate::error::AudioError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ControlSnapshot {
    pub(crate) stopped: bool,
    pub(crate) paused: bool,
    pub(crate) finished: bool,
}

#[derive(Default)]
pub struct PlaybackControl {
    stop: AtomicBool,
    pause: AtomicBool,
    finish: AtomicBool,
    changed: Notify,
    worker: OnceLock<Thread>,
}

impl PlaybackControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) fn attach_worker(&self, worker: Thread) {
        self.worker.get_or_init(|| worker);
    }

    pub(crate) fn wake_worker(&self) {
        if let Some(worker) = self.worker.get() {
            worker.unpark();
        }
    }

    fn announce(&self) {
        self.changed.notify_waiters();
        self.wake_worker();
    }

    pub(crate) fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.announce();
    }

    pub(crate) fn pause(&self) {
        self.pause.store(true, Ordering::Release);
        self.announce();
    }

    pub(crate) fn resume(&self) {
        self.pause.store(false, Ordering::Release);
        self.announce();
    }

    pub(crate) fn finish(&self) {
        self.finish.store(true, Ordering::Release);
        self.announce();
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    pub(crate) fn is_paused(&self) -> bool {
        self.pause.load(Ordering::Acquire)
    }

    pub(crate) fn snapshot(&self) -> ControlSnapshot {
        ControlSnapshot {
            stopped: self.is_stopped(),
            paused: self.is_paused(),
            finished: self.finish.load(Ordering::Acquire),
        }
    }

    pub(crate) async fn changed_from(&self, seen: ControlSnapshot) -> ControlSnapshot {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let now = self.snapshot();
            if now != seen {
                return now;
            }
            notified.await;
        }
    }
}

#[derive(Clone)]
pub struct PlaybackHandle {
    controls: Arc<[Arc<PlaybackControl>]>,
}

impl PlaybackHandle {
    pub(crate) fn from_control(control: Arc<PlaybackControl>) -> Self {
        Self {
            controls: Arc::from([control]),
        }
    }

    pub(crate) fn merge(handles: impl IntoIterator<Item = PlaybackHandle>) -> Self {
        let controls: Vec<_> = handles
            .into_iter()
            .flat_map(|h| h.controls.iter().cloned().collect::<Vec<_>>())
            .collect();
        Self {
            controls: Arc::from(controls),
        }
    }

    pub fn stop(&self) {
        for control in self.controls.iter() {
            control.stop();
        }
    }

    pub(crate) fn pause(&self) {
        for control in self.controls.iter() {
            control.pause();
        }
    }

    pub(crate) fn resume(&self) {
        for control in self.controls.iter() {
            control.resume();
        }
    }
}

type PlaybackCompletion = Pin<Box<dyn Future<Output = Result<(), AudioError>> + Send>>;

pub struct ControlledPlayback {
    playback: PlaybackHandle,
    completion: PlaybackCompletion,
    settled: bool,
}

impl ControlledPlayback {
    pub fn new(
        control: Arc<PlaybackControl>,
        completion: impl Future<Output = Result<(), AudioError>> + Send + 'static,
    ) -> Self {
        Self::merged(PlaybackHandle::from_control(control), completion)
    }

    pub(crate) fn from_task(
        control: Arc<PlaybackControl>,
        task: tokio::task::JoinHandle<Result<(), AudioError>>,
    ) -> Self {
        Self::new(control, async move {
            task.await
                .map_err(|e| AudioError::JoinFailed(e.to_string()))?
        })
    }

    pub(crate) fn merged(
        playback: PlaybackHandle,
        completion: impl Future<Output = Result<(), AudioError>> + Send + 'static,
    ) -> Self {
        Self {
            playback,
            completion: Box::pin(completion),
            settled: false,
        }
    }

    pub fn handle(&self) -> PlaybackHandle {
        self.playback.clone()
    }

    pub fn stop(&self) {
        self.playback.stop();
    }

    pub fn pause(&self) {
        self.playback.pause();
    }

    pub fn resume(&self) {
        self.playback.resume();
    }
}

impl Future for ControlledPlayback {
    type Output = Result<(), AudioError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let outcome = this.completion.as_mut().poll(cx);
        this.settled = outcome.is_ready();
        outcome
    }
}

impl Drop for ControlledPlayback {
    fn drop(&mut self) {
        if !self.settled {
            self.playback.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::poll;

    use super::*;

    const DEVICE_REFUSAL: &str = "no output device accepted the stream";

    #[tokio::test]
    async fn a_controlled_playback_carries_the_verdict_its_device_task_returned() {
        let played = ControlledPlayback::from_task(
            PlaybackControl::new(),
            tokio::spawn(async { Ok::<(), AudioError>(()) }),
        );
        assert!(
            played.await.is_ok(),
            "a device that played the clip to the end must finish the utterance"
        );

        let refused = ControlledPlayback::from_task(
            PlaybackControl::new(),
            tokio::spawn(async { Err(AudioError::Host(DEVICE_REFUSAL.to_owned())) }),
        );
        let outcome = refused.await;
        assert!(
            matches!(&outcome, Err(AudioError::Host(reason)) if reason == DEVICE_REFUSAL),
            "a device that never opened must reach the caller, got {outcome:?}"
        );
    }

    #[tokio::test]
    async fn a_device_task_that_died_reports_a_join_failure_instead_of_success() {
        let task = tokio::spawn(std::future::pending::<Result<(), AudioError>>());
        task.abort();

        let outcome = ControlledPlayback::from_task(PlaybackControl::new(), task).await;

        assert!(
            matches!(&outcome, Err(AudioError::JoinFailed(_))),
            "a task that never reported must not count as played, got {outcome:?}"
        );
    }

    fn playback_ending(
        verdict: Option<Result<(), AudioError>>,
    ) -> (Arc<PlaybackControl>, ControlledPlayback) {
        let control = PlaybackControl::new();
        let playback = ControlledPlayback::new(Arc::clone(&control), async move {
            match verdict {
                Some(verdict) => verdict,
                None => std::future::pending().await,
            }
        });
        (control, playback)
    }

    #[tokio::test]
    async fn dropping_a_playback_stops_it_unless_it_already_settled() {
        for (case, verdict, polled, stop_expected) in [
            ("never awaited and still playing", None, false, true),
            ("awaited and still playing", None, true, true),
            (
                "never awaited though it would finish",
                Some(Ok(())),
                false,
                true,
            ),
            ("settled as played", Some(Ok(())), true, false),
            (
                "settled as failed",
                Some(Err(AudioError::Host(DEVICE_REFUSAL.to_owned()))),
                true,
                false,
            ),
        ] {
            let (control, mut playback) = playback_ending(verdict);
            if polled {
                let settled = poll!(&mut playback).is_ready();
                assert_eq!(settled, !stop_expected, "{case}: wrong poll outcome");
            }

            drop(playback);

            assert_eq!(control.is_stopped(), stop_expected, "{case}");
        }
    }
}
