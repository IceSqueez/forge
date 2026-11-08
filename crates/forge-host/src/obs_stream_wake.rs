use std::sync::Arc;

use forge_events::{EventSource, EventsError};
use forge_obs::vocabulary::event_kinds::STREAMING_STARTED;
use forge_platform_youtube::BroadcastLookupWake;
use forge_runtime::EventBus;
use tokio::task::JoinHandle;

pub(crate) fn spawn_obs_stream_wake(
    bus: &Arc<EventBus>,
    wake: BroadcastLookupWake,
) -> JoinHandle<()> {
    let mut events = bus.subscribe();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event)
                    if event.source == EventSource::Obs
                        && event.kind == STREAMING_STARTED
                        && !event.replay =>
                {
                    wake.wake();
                }
                Ok(_) | Err(EventsError::LaggingReceiver) => {}
                Err(EventsError::BusClosed) => break,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use forge_events::Event;
    use forge_platform_core::{PlatformEndpoints, TokenSource};
    use forge_platform_youtube::endpoint_surfaces::{ENDPOINT_SURFACES, YOUTUBE_DATA_API};
    use forge_platform_youtube::{
        ActiveBroadcastIdHandle, ChatPollTimings, ChatPollerStop, LiveChatIdHandle, SharedQuota,
        YouTubeChatPoller, YouTubeDataApi,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    const HANG_GUARD: Duration = Duration::from_secs(30);
    const NEVER_WITHIN_THE_TEST: Duration = Duration::from_secs(3600);
    const HEADER_END: &[u8] = b"\r\n\r\n";
    const NO_ACTIVE_BROADCAST: &str = "{\"items\":[]}";

    async fn offline_youtube() -> (String, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let lookups = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&lookups);
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let counted = Arc::clone(&counted);
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut chunk = [0_u8; 1024];
                    while !head.windows(HEADER_END.len()).any(|w| w == HEADER_END) {
                        match stream.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(read) => head.extend_from_slice(&chunk[..read]),
                        }
                    }
                    counted.fetch_add(1, Ordering::SeqCst);
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{NO_ACTIVE_BROADCAST}",
                        NO_ACTIVE_BROADCAST.len()
                    );
                    if stream.write_all(reply.as_bytes()).await.is_ok() {
                        stream.shutdown().await.ok();
                    }
                });
            }
        });
        (base, lookups)
    }

    fn poller_at(base: &str, wake: &BroadcastLookupWake) -> YouTubeChatPoller {
        let endpoints = PlatformEndpoints::resolve(ENDPOINT_SURFACES, |variable| {
            (variable == YOUTUBE_DATA_API.env_var()).then(|| OsString::from(base))
        })
        .unwrap();
        let api = YouTubeDataApi::new(
            &endpoints,
            SharedQuota::default(),
            Arc::new(forge_events::test_support::NullPublisher),
        )
        .unwrap();
        let token: TokenSource = Arc::new(|| Box::pin(async { Ok("test-token".to_owned()) }));
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        YouTubeChatPoller::new(
            api,
            token,
            tx,
            "UCtest".to_owned(),
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            Arc::new(forge_storage::MockBanLedgerRepo::new()),
        )
        .with_timings(ChatPollTimings {
            offline_cadence: NEVER_WITHIN_THE_TEST,
            wake_follow_up_interval: NEVER_WITHIN_THE_TEST,
            ..ChatPollTimings::DEFAULT
        })
        .with_lookup_wake(wake.clone())
    }

    async fn lookups_reach(lookups: &AtomicUsize, count: usize) -> bool {
        tokio::time::timeout(HANG_GUARD, async {
            while lookups.load(Ordering::SeqCst) < count {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .is_ok()
    }

    #[tokio::test]
    async fn obs_starting_to_stream_wakes_the_offline_broadcast_lookup() {
        let (base, lookups) = offline_youtube().await;
        let wake = BroadcastLookupWake::new();
        let bus = EventBus::new(Arc::new(forge_runtime::NullEventLogRepo));
        let relay = spawn_obs_stream_wake(&bus, wake.clone());
        let stop = ChatPollerStop::new();
        let poller = tokio::spawn(poller_at(&base, &wake).run_until_stopped(stop.clone()));
        let first_lookup = lookups_reach(&lookups, 1).await;

        bus.publish(Event::new(
            EventSource::Obs,
            STREAMING_STARTED,
            serde_json::json!({}),
        ));
        let woken_lookup = lookups_reach(&lookups, 2).await;
        stop.stop();
        relay.abort();

        assert_eq!(
            (first_lookup, woken_lookup, poller.await.unwrap().is_ok()),
            (true, true, true)
        );
    }
}
