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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    use forge_obs::SwitchableObsSink;
    use forge_platform_core::ViewerReport;
    use tokio_stream::StreamExt as _;

    use crate::live_viewers::spawn_live_viewer_aggregator;
    use crate::test_support::channel_viewer_source;

    fn idle_obs() -> (Arc<SwitchableObsSink>, StreamOutputActive) {
        let sink = SwitchableObsSink::new();
        let output = sink.stream_output();
        (sink, output)
    }

    fn sources_of(items: &[LiveSource]) -> BTreeSet<LiveSource> {
        items.iter().copied().collect()
    }

    fn platforms_of(items: &[PlatformId]) -> LivePlatforms {
        items.iter().copied().collect()
    }

    async fn settle_to(handle: &mut StreamLiveHandle, expected: BTreeSet<LiveSource>) {
        if handle.current().sources() == &expected {
            return;
        }
        let outcome = tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(state) = handle.changed().await {
                if state.sources() == &expected {
                    return;
                }
            }
            panic!("stream-live signal ended before reaching {expected:?}");
        })
        .await;
        assert!(outcome.is_ok(), "timed out waiting for {expected:?}");
    }

    async fn let_spawned_tasks_run() {
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn live_report_marks_stream_live_with_the_registered_platform() {
        let viewers = spawn_live_viewer_aggregator();
        let (_obs_sink, obs) = idle_obs();
        let mut live = spawn_stream_live_signal(&viewers, obs);
        let (source, reports) = channel_viewer_source();
        viewers.register(PlatformId::Twitch, source);

        reports.send(ViewerReport::Live { count: 3 }).unwrap();

        settle_to(
            &mut live,
            sources_of(&[LiveSource::Platform(PlatformId::Twitch)]),
        )
        .await;
    }

    #[tokio::test]
    async fn absent_report_from_the_only_live_platform_clears_stream_live() {
        let viewers = spawn_live_viewer_aggregator();
        let (_obs_sink, obs) = idle_obs();
        let mut live = spawn_stream_live_signal(&viewers, obs);
        let (source, reports) = channel_viewer_source();
        viewers.register(PlatformId::Kick, source);
        reports.send(ViewerReport::Live { count: 3 }).unwrap();
        settle_to(
            &mut live,
            sources_of(&[LiveSource::Platform(PlatformId::Kick)]),
        )
        .await;

        reports.send(ViewerReport::Absent).unwrap();
        settle_to(&mut live, BTreeSet::new()).await;

        assert!(!live.current().is_live());
    }

    #[tokio::test]
    async fn stream_stays_live_on_the_remaining_platform_when_another_drops() {
        let viewers = spawn_live_viewer_aggregator();
        let (_obs_sink, obs) = idle_obs();
        let mut live = spawn_stream_live_signal(&viewers, obs);
        let (twitch, twitch_reports) = channel_viewer_source();
        let (youtube, youtube_reports) = channel_viewer_source();
        viewers.register(PlatformId::Twitch, twitch);
        viewers.register(PlatformId::YouTube, youtube);
        twitch_reports
            .send(ViewerReport::Live { count: 1 })
            .unwrap();
        youtube_reports
            .send(ViewerReport::Live { count: 2 })
            .unwrap();
        settle_to(
            &mut live,
            sources_of(&[
                LiveSource::Platform(PlatformId::Twitch),
                LiveSource::Platform(PlatformId::YouTube),
            ]),
        )
        .await;

        drop(youtube_reports);

        settle_to(
            &mut live,
            sources_of(&[LiveSource::Platform(PlatformId::Twitch)]),
        )
        .await;
    }

    #[test]
    fn obs_stream_output_counts_as_a_live_source_alongside_platforms() {
        let obs_only = StreamLiveState::from_parts(&LivePlatforms::new(), true);
        let both = StreamLiveState::from_parts(&platforms_of(&[PlatformId::YouTube]), true);

        assert_eq!(
            (obs_only.is_live(), both.sources().clone()),
            (
                true,
                sources_of(&[
                    LiveSource::Platform(PlatformId::YouTube),
                    LiveSource::ObsStreamOutput,
                ]),
            ),
        );
    }

    #[test]
    fn obs_output_off_with_no_platforms_is_not_live() {
        assert!(!StreamLiveState::from_parts(&LivePlatforms::new(), false).is_live());
    }

    #[tokio::test]
    async fn republishing_an_unchanged_platform_set_emits_nothing() {
        let (platforms_tx, platforms_rx) = watch::channel(LivePlatforms::new());
        let (_obs_sink, obs) = idle_obs();
        let (output_tx, mut output_rx) = watch::channel(StreamLiveState::default());
        tokio::spawn(follow_live_inputs(platforms_rx, obs, output_tx));
        platforms_tx
            .send(platforms_of(&[PlatformId::Twitch]))
            .unwrap();
        output_rx.changed().await.unwrap();
        output_rx.borrow_and_update();

        platforms_tx
            .send(platforms_of(&[PlatformId::Twitch]))
            .unwrap();
        let_spawned_tasks_run().await;

        assert!(!output_rx.has_changed().unwrap());
    }

    #[tokio::test]
    async fn signal_starts_from_platforms_already_live_at_spawn() {
        let viewers = spawn_live_viewer_aggregator();
        let mut platforms = viewers.live_platforms();
        let (source, reports) = channel_viewer_source();
        viewers.register(PlatformId::Twitch, source);
        reports.send(ViewerReport::Live { count: 7 }).unwrap();
        platforms.changed().await.unwrap();
        let (_obs_sink, obs) = idle_obs();

        let live = spawn_stream_live_signal(&viewers, obs);

        assert_eq!(
            live.current().sources(),
            &sources_of(&[LiveSource::Platform(PlatformId::Twitch)]),
        );
    }

    #[tokio::test]
    async fn new_subscriber_first_receives_the_current_state() {
        let viewers = spawn_live_viewer_aggregator();
        let (_obs_sink, obs) = idle_obs();
        let mut live = spawn_stream_live_signal(&viewers, obs);
        let (source, reports) = channel_viewer_source();
        viewers.register(PlatformId::YouTube, source);
        reports.send(ViewerReport::Live { count: 4 }).unwrap();
        let expected = sources_of(&[LiveSource::Platform(PlatformId::YouTube)]);
        settle_to(&mut live, expected.clone()).await;

        let mut subscriber = Box::pin(live.subscribe());
        let first = tokio::time::timeout(Duration::from_secs(2), subscriber.next())
            .await
            .expect("subscriber got no initial value");

        assert_eq!(first.map(|state| state.sources().clone()), Some(expected));
    }

    #[tokio::test]
    async fn closed_platform_feed_drops_its_stale_platforms() {
        let (platforms_tx, platforms_rx) = watch::channel(LivePlatforms::new());
        let (_obs_sink, obs) = idle_obs();
        let (output_tx, output_rx) = watch::channel(StreamLiveState::default());
        let mut live = StreamLiveHandle(output_rx);
        tokio::spawn(follow_live_inputs(platforms_rx, obs, output_tx));
        platforms_tx
            .send(platforms_of(&[PlatformId::Kick]))
            .unwrap();
        settle_to(
            &mut live,
            sources_of(&[LiveSource::Platform(PlatformId::Kick)]),
        )
        .await;

        drop(platforms_tx);

        settle_to(&mut live, BTreeSet::new()).await;
    }

    #[tokio::test]
    async fn platform_changes_keep_flowing_after_the_obs_feed_closes() {
        let (platforms_tx, platforms_rx) = watch::channel(LivePlatforms::new());
        let (obs_sink, obs) = idle_obs();
        let (output_tx, output_rx) = watch::channel(StreamLiveState::default());
        let mut live = StreamLiveHandle(output_rx);
        tokio::spawn(follow_live_inputs(platforms_rx, obs, output_tx));
        drop(obs_sink);
        let_spawned_tasks_run().await;

        platforms_tx
            .send(platforms_of(&[PlatformId::Twitch]))
            .unwrap();

        settle_to(
            &mut live,
            sources_of(&[LiveSource::Platform(PlatformId::Twitch)]),
        )
        .await;
    }

    #[tokio::test]
    async fn signal_ends_once_every_input_feed_has_closed() {
        let (platforms_tx, platforms_rx) = watch::channel(LivePlatforms::new());
        let (obs_sink, obs) = idle_obs();
        let (output_tx, output_rx) = watch::channel(StreamLiveState::default());
        let mut live = StreamLiveHandle(output_rx);
        tokio::spawn(follow_live_inputs(platforms_rx, obs, output_tx));

        drop(platforms_tx);
        drop(obs_sink);

        let ended = tokio::time::timeout(Duration::from_secs(2), async {
            while live.changed().await.is_some() {}
        })
        .await;
        assert!(ended.is_ok());
    }
}
