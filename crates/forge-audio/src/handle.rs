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
pub(crate) struct PlaybackControl {
    stop: AtomicBool,
    pause: AtomicBool,
    finish: AtomicBool,
    changed: Notify,
    worker: OnceLock<Thread>,
}

impl PlaybackControl {
    pub(crate) fn new() -> Arc<Self> {
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

#[derive(Clone, Default)]
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

    pub fn pause(&self) {
        for control in self.controls.iter() {
            control.pause();
        }
    }

    pub fn resume(&self) {
        for control in self.controls.iter() {
            control.resume();
        }
    }

    pub async fn finished(&self) {
        for control in self.controls.iter() {
            loop {
                let seen = control.snapshot();
                if seen.finished {
                    break;
                }
                control.changed_from(seen).await;
            }
        }
    }
}

enum Completion {
    Ready,
    Handle(tokio::task::JoinHandle<Result<(), AudioError>>),
    Merged(Pin<Box<dyn Future<Output = Result<(), AudioError>> + Send>>),
}

pub struct ControlledPlayback {
    playback: PlaybackHandle,
    completion: Completion,
}

impl ControlledPlayback {
    pub fn resolved(outcome: Result<(), AudioError>) -> Self {
        Self::from_future(std::future::ready(outcome))
    }

    pub fn from_future(
        completion: impl Future<Output = Result<(), AudioError>> + Send + 'static,
    ) -> Self {
        Self::merged(PlaybackHandle::default(), Box::pin(completion))
    }

    pub(crate) fn completed() -> Self {
        Self {
            playback: PlaybackHandle::default(),
            completion: Completion::Ready,
        }
    }

    pub(crate) fn from_handle(
        playback: PlaybackHandle,
        completion: tokio::task::JoinHandle<Result<(), AudioError>>,
    ) -> Self {
        Self {
            playback,
            completion: Completion::Handle(completion),
        }
    }

    pub(crate) fn merged(
        playback: PlaybackHandle,
        completion: Pin<Box<dyn Future<Output = Result<(), AudioError>> + Send>>,
    ) -> Self {
        Self {
            playback,
            completion: Completion::Merged(completion),
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
        match &mut this.completion {
            Completion::Ready => Poll::Ready(Ok(())),
            Completion::Handle(handle) => Pin::new(handle).poll(cx).map(|res| {
                res.map_err(|e| AudioError::JoinFailed(e.to_string()))
                    .and_then(|played| played)
            }),
            Completion::Merged(future) => future.as_mut().poll(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICE_REFUSAL: &str = "no output device accepted the stream";

    #[tokio::test]
    async fn a_controlled_playback_carries_the_verdict_its_device_task_returned() {
        let played = ControlledPlayback::from_handle(
            PlaybackHandle::default(),
            tokio::spawn(async { Ok::<(), AudioError>(()) }),
        );
        assert!(
            played.await.is_ok(),
            "a device that played the clip to the end must finish the utterance"
        );

        let refused = ControlledPlayback::from_handle(
            PlaybackHandle::default(),
            tokio::spawn(async { Err(AudioError::Host(DEVICE_REFUSAL.to_owned())) }),
        );
        let outcome = refused.await;
        assert!(
            matches!(&outcome, Err(AudioError::Host(reason)) if reason == DEVICE_REFUSAL),
            "a device that never opened must reach the caller, got {outcome:?}"
        );
    }
}
