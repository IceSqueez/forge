use std::sync::Arc;
use std::time::Duration;

use forge_net::{TaskOwner, TaskShutdown};
use forge_platform_core::{LiveViewerSource, ViewerReportStream};
use forge_runtime::{
    EventBus, NotificationPreferences, NullEventLogRepo, TwitchEmoteLexicon,
    spawn_event_log_bridge, spawn_live_viewer_aggregator, spawn_notification_policy,
    spawn_stream_live_signal, spawn_twitch_emote_learning,
};
use forge_types::PlatformId;
use tokio::sync::watch;

const GRACE: Duration = Duration::from_secs(1);

async fn settle() {
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
}

struct SilentSource;

impl LiveViewerSource for SilentSource {
    fn viewer_reports(&self) -> ViewerReportStream {
        Box::pin(futures_util::stream::pending())
    }
}

#[tokio::test(start_paused = true)]
async fn the_bus_observers_end_on_stop_while_the_bus_stays_open() {
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let tasks = TaskOwner::new();
    spawn_event_log_bridge(Arc::clone(&bus), &tasks);
    let policy = spawn_notification_policy(&bus, NotificationPreferences::default(), &tasks);
    spawn_twitch_emote_learning(&bus, TwitchEmoteLexicon::default(), &tasks);
    settle().await;

    let stopped = tasks.shutdown(GRACE).await;

    assert_eq!(stopped, TaskShutdown::Drained);
    drop((bus, policy));
}

#[tokio::test(start_paused = true)]
async fn the_viewer_aggregator_its_forwarders_and_the_live_signal_end_on_stop() {
    let tasks = TaskOwner::new();
    let viewers = spawn_live_viewer_aggregator(&tasks);
    viewers.register(PlatformId::Twitch, Box::new(SilentSource));
    let (_obs_tx, obs) = watch::channel(false);
    let live = spawn_stream_live_signal(&viewers, obs, &tasks);
    settle().await;

    let stopped = tasks.shutdown(GRACE).await;

    assert_eq!(stopped, TaskShutdown::Drained);
    drop((viewers, live));
}
