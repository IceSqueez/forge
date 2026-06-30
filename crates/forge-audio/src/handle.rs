use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cancellation token for one or more in-flight clips.
///
/// `stop` is cooperative: cpal output goes silent at the next device callback
/// boundary (sub-buffer audio already handed to the driver still drains, a
/// low-tens-of-ms tail) and the playback thread tears the stream down within one
/// poll interval. A handle returned by a sink that does not implement
/// cancellation (the `AudioSink::play_stoppable` default) carries no flags —
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
