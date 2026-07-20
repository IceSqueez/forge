use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use crate::error::AudioError;

/// Cancellation/pause token for one or more in-flight clips.
///
/// `stop` is cooperative: cpal output goes silent at the next device callback
/// boundary (sub-buffer audio already handed to the driver still drains, a
/// low-tens-of-ms tail) and the playback thread tears the stream down within one
/// poll interval. `pause`/`resume` hold the writer on silence without consuming
/// buffered samples, so playback resumes from the exact spot; `stop` always wins
/// over an active pause. A handle returned by a sink that does not implement
/// cancellation (the `AudioSink::play_stoppable` default) carries no flags -
/// `stop`/`pause`/`resume` are then no-ops and the clip runs to completion.
#[derive(Clone, Default)]
pub struct PlaybackHandle {
    stop_flags: Arc<[Arc<AtomicBool>]>,
    pause_flags: Arc<[Arc<AtomicBool>]>,
}

impl PlaybackHandle {
    pub(crate) fn from_flags(stop: Arc<AtomicBool>, pause: Arc<AtomicBool>) -> Self {
        Self {
            stop_flags: Arc::from([stop]),
            pause_flags: Arc::from([pause]),
        }
    }

    pub(crate) fn merge(handles: impl IntoIterator<Item = PlaybackHandle>) -> Self {
        let mut stop_flags = Vec::new();
        let mut pause_flags = Vec::new();
        for h in handles {
            stop_flags.extend(h.stop_flags.iter().cloned());
            pause_flags.extend(h.pause_flags.iter().cloned());
        }
        Self {
            stop_flags: Arc::from(stop_flags),
            pause_flags: Arc::from(pause_flags),
        }
    }

    pub fn stop(&self) {
        for flag in self.stop_flags.iter() {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn pause(&self) {
        for flag in self.pause_flags.iter() {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn resume(&self) {
        for flag in self.pause_flags.iter() {
            flag.store(false, Ordering::Relaxed);
        }
    }
}

enum Completion {
    Ready,
    Handle(tokio::task::JoinHandle<()>),
}

pub struct ControlledPlayback {
    playback: PlaybackHandle,
    completion: Completion,
}

impl ControlledPlayback {
    pub(crate) fn completed() -> Self {
        Self {
            playback: PlaybackHandle::default(),
            completion: Completion::Ready,
        }
    }

    pub(crate) fn from_handle(
        playback: PlaybackHandle,
        completion: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self {
            playback,
            completion: Completion::Handle(completion),
        }
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
            Completion::Handle(handle) => Pin::new(handle)
                .poll(cx)
                .map(|res| res.map_err(|e| AudioError::JoinFailed(e.to_string()))),
        }
    }
}
