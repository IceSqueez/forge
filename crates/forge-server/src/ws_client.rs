use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64},
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use time::OffsetDateTime;
use tokio::time::Instant;

use crate::bus_adapter::ClientId;

const EVENTS_WINDOW_SECS: u64 = 10;
const RECENT_EVENTS_CAP: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientType {
    Unknown,
    ObsBrowser,
    Ios,
    Android,
    StreamDeck,
    ThirdParty(String),
}

pub fn detect_from_user_agent(ua: Option<&str>) -> ClientType {
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

pub struct WsClient {
    pub id: ClientId,
    pub identification: ArcSwap<String>,
    pub remote_addr: SocketAddr,
    pub client_type: ArcSwap<ClientType>,
    pub authenticated: AtomicBool,
    pub connected_at: OffsetDateTime,
    pub bytes_sent_session: AtomicU64,
    pub recent_events: Mutex<VecDeque<Instant>>,
    pub drop_counter: Arc<AtomicU64>,
}

impl WsClient {
    pub fn new(id: ClientId, remote_addr: SocketAddr, drop_counter: Arc<AtomicU64>) -> Self {
        Self {
            identification: ArcSwap::from_pointee(remote_addr.to_string()),
            id,
            remote_addr,
            client_type: ArcSwap::from_pointee(ClientType::Unknown),
            authenticated: AtomicBool::new(false),
            connected_at: OffsetDateTime::now_utc(),
            bytes_sent_session: AtomicU64::new(0),
            recent_events: Mutex::new(VecDeque::new()),
            drop_counter,
        }
    }

    pub fn uptime(&self) -> time::Duration {
        (OffsetDateTime::now_utc() - self.connected_at).abs()
    }

    pub fn events_per_second(&self) -> f32 {
        let Ok(guard) = self.recent_events.lock() else {
            return 0.0;
        };
        Self::compute_eps(&guard)
    }

    pub fn record_event(&self) -> f32 {
        let now = Instant::now();
        let Ok(mut guard) = self.recent_events.lock() else {
            return 0.0;
        };
        guard.push_back(now);
        while guard.len() > RECENT_EVENTS_CAP {
            guard.pop_front();
        }
        Self::compute_eps(&guard)
    }

    fn compute_eps(deque: &VecDeque<Instant>) -> f32 {
        let now = Instant::now();
        let window = Duration::from_secs(EVENTS_WINDOW_SECS);
        let cutoff = now.checked_sub(window).unwrap_or(now);
        deque.iter().filter(|&&t| t >= cutoff).count() as f32 / EVENTS_WINDOW_SECS as f32
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;

    fn make_client() -> WsClient {
        let drop_counter = Arc::new(AtomicU64::new(0));
        WsClient::new(
            ClientId::next(),
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

    #[test]
    fn record_event_trims_to_100() {
        let client = make_client();
        for _ in 0..101 {
            client.record_event();
        }
        let len = client.recent_events.lock().unwrap().len();
        assert_eq!(len, 100);
    }
}
