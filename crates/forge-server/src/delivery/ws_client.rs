use forge_types::RecoverMutex;
use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use time::OffsetDateTime;
use tokio::time::Instant;

use crate::delivery::bus_adapter::ClientId;
use crate::delivery::rate_window::RateWindow;

const EVENTS_WINDOW: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClientType {
    Unknown,
    ObsBrowser,
    Ios,
    Android,
    StreamDeck,
}

impl ClientType {
    pub(crate) fn type_str(&self) -> &'static str {
        match self {
            ClientType::Unknown => "unknown",
            ClientType::ObsBrowser => "obs_browser",
            ClientType::Ios => "ios",
            ClientType::Android => "android",
            ClientType::StreamDeck => "stream_deck",
        }
    }
}

pub(crate) fn detect_from_user_agent(ua: Option<&str>) -> ClientType {
    let Some(ua) = ua else {
        return ClientType::Unknown;
    };
    if ua.contains("OBS") {
        ClientType::ObsBrowser
    } else if ua.contains("iOS") {
        ClientType::Ios
    } else if ua.contains("Android") {
        ClientType::Android
    } else if ua.contains("Elgato Stream Deck") {
        ClientType::StreamDeck
    } else {
        ClientType::Unknown
    }
}

const NOT_AUTHENTICATED: u64 = u64::MAX;

pub(crate) struct WsClient {
    pub id: ClientId,
    pub identification: ArcSwap<String>,
    pub remote_addr: SocketAddr,
    pub client_type: ArcSwap<ClientType>,
    bearer_generation: AtomicU64,
    pub connected_at: OffsetDateTime,
    pub bytes_sent_session: AtomicU64,
    recent_events: Mutex<RateWindow>,
    pub drop_counter: Arc<AtomicU64>,
}

impl WsClient {
    pub(crate) fn new(id: ClientId, remote_addr: SocketAddr, drop_counter: Arc<AtomicU64>) -> Self {
        Self {
            identification: ArcSwap::from_pointee(remote_addr.to_string()),
            id,
            remote_addr,
            client_type: ArcSwap::from_pointee(ClientType::Unknown),
            bearer_generation: AtomicU64::new(NOT_AUTHENTICATED),
            connected_at: OffsetDateTime::now_utc(),
            bytes_sent_session: AtomicU64::new(0),
            recent_events: Mutex::new(RateWindow::new(EVENTS_WINDOW)),
            drop_counter,
        }
    }

    pub(crate) fn mark_bearer_authenticated(&self, generation: u64) {
        self.bearer_generation.store(generation, Ordering::SeqCst);
    }

    pub(crate) fn bearer_generation(&self) -> Option<u64> {
        let generation = self.bearer_generation.load(Ordering::SeqCst);
        (generation != NOT_AUTHENTICATED).then_some(generation)
    }

    pub(crate) fn uptime(&self) -> time::Duration {
        (OffsetDateTime::now_utc() - self.connected_at).abs()
    }

    pub(crate) fn events_per_second(&self) -> f32 {
        let events = self.recent_events.lock_recover().total_at(Instant::now());
        events as f32 / EVENTS_WINDOW.as_secs_f32()
    }

    pub(crate) fn record_event(&self) {
        let now = Instant::now();
        let mut window = self.recent_events.lock_recover();
        window.push(now, 1);
        window.total_at(now);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;

    const EVENTS_WINDOW_SECS: u64 = EVENTS_WINDOW.as_secs();

    fn make_client() -> WsClient {
        let drop_counter = Arc::new(AtomicU64::new(0));
        WsClient::new(
            ClientId::new(0),
            "127.0.0.1:0".parse().unwrap(),
            drop_counter,
        )
    }

    #[test]
    fn events_per_second_returns_zero_on_empty() {
        let client = make_client();
        assert_eq!(client.events_per_second(), 0.0);
    }

    #[tokio::test]
    async fn events_per_second_rolling_10s_window() {
        tokio::time::pause();
        let client = make_client();
        for _ in 0..10 {
            client.record_event();
        }
        tokio::time::advance(Duration::from_secs(5)).await;
        let eps = client.events_per_second();
        assert!((eps - 1.0).abs() < 0.001, "expected ~1.0 ev/s, got {eps}");
    }

    #[tokio::test]
    async fn events_sitting_exactly_on_the_window_edge_still_count() {
        tokio::time::pause();
        let client = make_client();
        for _ in 0..10 {
            client.record_event();
        }
        tokio::time::advance(Duration::from_secs(EVENTS_WINDOW_SECS)).await;
        let eps = client.events_per_second();
        assert!(
            (eps - 1.0).abs() < 0.001,
            "the window edge is inclusive, got {eps}"
        );
    }

    #[tokio::test]
    async fn events_older_than_the_window_stop_counting() {
        tokio::time::pause();
        let client = make_client();
        for _ in 0..10 {
            client.record_event();
        }
        tokio::time::advance(Duration::from_secs(EVENTS_WINDOW_SECS) + Duration::from_millis(1))
            .await;
        assert_eq!(client.events_per_second(), 0.0);
    }

    #[test]
    fn detect_from_user_agent_coverage() {
        assert_eq!(
            detect_from_user_agent(Some("OBS/30.0 (Linux)")),
            ClientType::ObsBrowser
        );
        assert_eq!(
            detect_from_user_agent(Some("ForgeOverlay/1.0 iOS/17.0")),
            ClientType::Ios
        );
        assert_eq!(
            detect_from_user_agent(Some("ForgeOverlay/1.0 Android/14")),
            ClientType::Android
        );
        assert_eq!(
            detect_from_user_agent(Some("Elgato Stream Deck 6.7")),
            ClientType::StreamDeck
        );
        assert_eq!(
            detect_from_user_agent(Some("Mozilla/5.0 (compatible)")),
            ClientType::Unknown
        );
        assert_eq!(detect_from_user_agent(None), ClientType::Unknown);
    }

    #[tokio::test]
    async fn a_burst_above_ten_events_per_second_is_reported_in_full() {
        tokio::time::pause();
        let client = make_client();

        for _ in 0..EVENTS_WINDOW_SECS * 25 {
            client.record_event();
        }

        let reported = client.events_per_second();
        assert!(
            (reported - 25.0).abs() < 0.001,
            "expected 25 ev/s, got {reported}"
        );
    }

    #[tokio::test]
    async fn record_event_forgets_events_that_left_the_window() {
        tokio::time::pause();
        let client = make_client();
        for _ in 0..10 {
            client.record_event();
        }
        tokio::time::advance(Duration::from_secs(EVENTS_WINDOW_SECS) + Duration::from_millis(1))
            .await;

        client.record_event();

        assert_eq!(client.recent_events.lock().unwrap().retained(), 1);
    }
}
