use std::sync::Arc;

use crate::fan_out::FanOutSink;
use crate::remote::{PlaybackCorrelation, RemoteAudioDestination, RemoteDestinationId};
use crate::remote_sink::RemoteSink;
use crate::sink::AudioSink;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackTarget {
    pub destination: RemoteDestinationId,
    pub correlation: PlaybackCorrelation,
}

/// Builds the legs of one playback aimed at a single destination; the global route's remote leg
/// is never among them.
pub trait TargetedSinkFactory: Send + Sync {
    fn sink_for(&self, target: &PlaybackTarget) -> Arc<dyn AudioSink>;
}

pub struct RemoteLegFactory {
    remote: Arc<dyn RemoteAudioDestination>,
    local: Option<Arc<dyn AudioSink>>,
}

impl RemoteLegFactory {
    /// `local` is the monitoring leg, present only when the global route plays locally.
    pub fn new(remote: Arc<dyn RemoteAudioDestination>, local: Option<Arc<dyn AudioSink>>) -> Self {
        Self { remote, local }
    }
}

impl TargetedSinkFactory for RemoteLegFactory {
    fn sink_for(&self, target: &PlaybackTarget) -> Arc<dyn AudioSink> {
        let remote: Arc<dyn AudioSink> = Arc::new(
            RemoteSink::new(Arc::clone(&self.remote), target.destination.clone())
                .correlated(target.correlation.clone()),
        );
        match &self.local {
            Some(local) => Arc::new(FanOutSink::new(vec![Arc::clone(local), remote])),
            None => remote,
        }
    }
}

