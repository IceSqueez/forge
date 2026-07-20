use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use crate::error::AudioError;

/// Cancellation token for one or more in-flight clips.
///
/// `stop` is cooperative: cpal output goes silent at the next device callback
/// boundary (sub-buffer audio already handed to the driver still drains, a
/// low-tens-of-ms tail) and the playback thread tears the stream down within one
/// poll interval. A handle returned by a sink that does not implement
/// cancellation (the `AudioSink::play_stoppable` default) carries no flags -
/// `stop` is then a no-op and the clip runs to completion.
#[derive(Clone, Default)]
pub struct PlaybackHandle {
    flags: Arc<[Arc<AtomicBool>]>,
}

impl PlaybackHandle {
    pub(crate) fn from_flag(flag: Arc<AtomicBool>) -> Self {
        Self {
            flags: Arc::from([flag]),
        }
    }

    pub(crate) fn merge(handles: impl IntoIterator<Item = PlaybackHandle>) -> Self {
        let flags: Vec<Arc<AtomicBool>> =
            handles.into_iter().flat_map(|h| h.flags.to_vec()).collect();
        Self {
            flags: Arc::from(flags),
        }
    }

    pub fn stop(&self) {
        for flag in self.flags.iter() {
            flag.store(true, Ordering::Relaxed);
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
