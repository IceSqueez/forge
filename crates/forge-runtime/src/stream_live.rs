use std::collections::BTreeSet;

use forge_obs::StreamOutputActive;
use forge_types::PlatformId;
use futures_core::Stream;
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;

use crate::live_viewers::{LivePlatforms, LiveViewerAggregatorHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LiveSource {
    Platform(PlatformId),
    ObsStreamOutput,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamLiveState {
    sources: BTreeSet<LiveSource>,
}

impl StreamLiveState {
    fn from_parts(platforms: &LivePlatforms, obs_streaming: bool) -> Self {
        let mut sources: BTreeSet<LiveSource> = platforms
            .iter()
            .copied()
            .map(LiveSource::Platform)
            .collect();
        if obs_streaming {
            sources.insert(LiveSource::ObsStreamOutput);
        }
        Self { sources }
    }

    pub fn is_live(&self) -> bool {
        !self.sources.is_empty()
    }

    pub fn sources(&self) -> &BTreeSet<LiveSource> {
        &self.sources
    }
}

#[derive(Debug, Clone)]
pub struct StreamLiveHandle(watch::Receiver<StreamLiveState>);

impl StreamLiveHandle {
    pub fn current(&self) -> StreamLiveState {
        self.0.borrow().clone()
    }

    pub async fn changed(&mut self) -> Option<StreamLiveState> {
        self.0.changed().await.ok()?;
        Some(self.0.borrow_and_update().clone())
    }

    pub fn subscribe(&self) -> impl Stream<Item = StreamLiveState> + Send + 'static {
        WatchStream::new(self.0.clone())
    }
}

pub fn spawn_stream_live_signal(
    viewers: &LiveViewerAggregatorHandle,
    obs: StreamOutputActive,
) -> StreamLiveHandle {
    let platforms = viewers.live_platforms();
    let initial = StreamLiveState::from_parts(&platforms.borrow(), obs.is_active());
    let (output, receiver) = watch::channel(initial);
    tokio::spawn(follow_live_inputs(platforms, obs, output));
    StreamLiveHandle(receiver)
}

async fn follow_live_inputs(
    mut platforms: watch::Receiver<LivePlatforms>,
    mut obs: StreamOutputActive,
    output: watch::Sender<StreamLiveState>,
) {
    let mut platforms_open = true;
    let mut obs_open = true;
    let mut obs_streaming = obs.is_active();
    while platforms_open || obs_open {
        tokio::select! {
            () = output.closed() => return,
            changed = platforms.changed(), if platforms_open => {
                platforms_open = changed.is_ok();
            }
            active = obs.changed(), if obs_open => match active {
                Some(active) => obs_streaming = active,
                None => {
                    obs_open = false;
                    obs_streaming = false;
                }
            },
        }
        let live_platforms = if platforms_open {
            platforms.borrow_and_update().clone()
        } else {
            LivePlatforms::new()
        };
        let next = StreamLiveState::from_parts(&live_platforms, obs_streaming);
        output.send_if_modified(|current| {
            if *current == next {
                false
            } else {
                *current = next;
                true
            }
        });
    }
}

