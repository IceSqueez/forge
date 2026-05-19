use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use async_trait::async_trait;
use forge_events::{Event, EventSource, EventsError};
use forge_platform_core::{PlatformError, RateLimitOutcome, RateLimiter};
use forge_runtime::EventBus;
use forge_storage::{CredentialId, CredentialsRepo};
use forge_types::OAuthToken;

pub struct ChatSendBridge {
    bus: Arc<EventBus>,
    creds: Arc<dyn CredentialsRepo>,
}

pub struct ChatSendBridgeHandle {
    cancel: Arc<AtomicBool>,
}

impl ChatSendBridgeHandle {
    pub fn shutdown(self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Clone for ChatSendBridgeHandle {
    fn clone(&self) -> Self {
        Self {
            cancel: Arc::clone(&self.cancel),
        }
    }
}

impl ChatSendBridge {
    pub fn spawn(bus: Arc<EventBus>, creds: Arc<dyn CredentialsRepo>) -> ChatSendBridgeHandle {
        let cancel = Arc::new(AtomicBool::new(false));
        let bridge = Self {
            bus: Arc::clone(&bus),
            creds,
        };
        tokio::spawn(bridge.run(Arc::clone(&cancel)));
        ChatSendBridgeHandle { cancel }
    }

    async fn run(self, cancel: Arc<AtomicBool>) {
        let mut sub = self.bus.subscribe();
        while !cancel.load(Ordering::Relaxed) {
            let event = match sub.recv().await {
                Ok(e) => e,
                Err(EventsError::BusClosed) => break,
                Err(EventsError::LaggingReceiver) => {
                    tracing::warn!("chat_send_bridge: lagging receiver, some events skipped");
                    continue;
                }
                Err(_) => continue,
            };

            if event.source != EventSource::Core || event.kind != "chat.send.request" {
                continue;
            }

            let target = match extract_target(&event) {
                Some(t) => t,
                None => continue,
            };
            if target != "twitch" {
                continue;
            }

            let message = match extract_message(&event) {
                Some(m) => m,
                None => continue,
            };

            let caused_by = event.id;

            match self.try_send(&message).await {
                Ok(()) => {
                    self.bus.publish(Event::caused_by(
                        EventSource::Twitch,
                        "chat.sent",
                        serde_json::json!({"target": "twitch"}),
                        caused_by,
                    ));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "chat send failed");
                    self.bus.publish(Event::caused_by(
                        EventSource::Twitch,
                        "chat.send.failed",
                        serde_json::json!({"target": "twitch", "error": e}),
                        caused_by,
                    ));
                }
            }
        }
    }

    async fn try_send(&self, message: &str) -> Result<(), String> {
        let cid =
            crate::auth::client_id().ok_or_else(|| "no Twitch client_id configured".to_string())?;

        let json_str = self
            .creds
            .load(&CredentialId::new("twitch:broadcaster"))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no twitch credentials stored".to_string())?;

        let bundle: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| e.to_string())?;

        let token = bundle
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or("missing access_token")?
            .to_owned();

        let user_id = bundle
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or("missing user_id")?
            .to_owned();

        crate::chat::send_chat(
            &NoopRateLimiter,
            &OAuthToken::new(token),
            &cid,
            &user_id,
            &user_id,
            message,
        )
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    }
}

fn extract_target(event: &Event) -> Option<String> {
    event
        .payload
        .get("target")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
}

fn extract_message(event: &Event) -> Option<String> {
    event
        .payload
        .get("message")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
}

struct NoopRateLimiter;

#[async_trait]
impl RateLimiter for NoopRateLimiter {
    async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
        Ok(RateLimitOutcome::Granted)
    }

    fn remaining(&self) -> u32 {
        u32::MAX
    }

    async fn observe_remote_throttle(&self, _retry_after: Duration) {}
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_runtime::NullEventLogRepo;
    use forge_storage_sqlite::SqliteBackend;
    use std::time::Duration;

    fn make_request_event(target: &str, message: &str) -> Event {
        Event::new(
            EventSource::Core,
            "chat.send.request",
            serde_json::json!({"target": target, "message": message}),
        )
    }

    #[test]
    fn extract_message_returns_value_from_payload() {
        let ev = make_request_event("twitch", "hello world");
        assert_eq!(extract_message(&ev).as_deref(), Some("hello world"));
    }

    #[test]
    fn extract_message_returns_none_when_field_missing() {
        let ev = Event::new(
            EventSource::Core,
            "chat.send.request",
            serde_json::json!({"target": "twitch"}),
        );
        assert!(extract_message(&ev).is_none());
    }

    #[test]
    fn extract_target_returns_value_from_payload() {
        let ev = make_request_event("twitch", "hi");
        assert_eq!(extract_target(&ev).as_deref(), Some("twitch"));
    }

    #[test]
    fn extract_target_returns_none_when_field_missing() {
        let ev = Event::new(
            EventSource::Core,
            "chat.send.request",
            serde_json::json!({"message": "hi"}),
        );
        assert!(extract_target(&ev).is_none());
    }

    #[tokio::test]
    async fn bridge_publishes_failed_event_when_no_credentials_stored() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let backend = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );

        let mut test_sub = bus.subscribe();
        ChatSendBridge::spawn(Arc::clone(&bus), backend as Arc<dyn CredentialsRepo>);
        tokio::task::yield_now().await;

        bus.publish(make_request_event("twitch", "hello"));

        let failed = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                match test_sub.recv().await {
                    Ok(e) if e.kind == "chat.send.failed" => return Some(e),
                    Ok(e) if e.kind == "chat.sent" => return Some(e),
                    Ok(_) => continue,
                    Err(_) => return None,
                }
            }
        })
        .await
        .unwrap();

        let ev = failed.unwrap();
        assert_eq!(ev.kind, "chat.send.failed");
        assert_eq!(ev.source, EventSource::Twitch);

        let error_msg = ev.payload["error"].as_str().unwrap_or("");
        assert!(
            error_msg.contains("no twitch credentials") || error_msg.contains("client_id"),
            "unexpected error: {error_msg}"
        );
    }

    #[tokio::test]
    async fn bridge_preserves_caused_by_chain() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let backend = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );

        let mut test_sub = bus.subscribe();
        ChatSendBridge::spawn(Arc::clone(&bus), backend as Arc<dyn CredentialsRepo>);
        tokio::task::yield_now().await;

        let request = make_request_event("twitch", "hi");
        let request_id = request.id;
        bus.publish(request);

        let result = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                match test_sub.recv().await {
                    Ok(e) if e.kind == "chat.send.failed" || e.kind == "chat.sent" => {
                        return Some(e);
                    }
                    Ok(_) => continue,
                    Err(_) => return None,
                }
            }
        })
        .await
        .unwrap()
        .unwrap();

        assert_eq!(
            result.caused_by,
            Some(request_id),
            "result event must carry caused_by = request event id"
        );
    }

    #[tokio::test]
    async fn bridge_ignores_non_twitch_target() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let backend = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );

        let mut test_sub = bus.subscribe();
        ChatSendBridge::spawn(Arc::clone(&bus), backend as Arc<dyn CredentialsRepo>);
        tokio::task::yield_now().await;

        bus.publish(make_request_event("youtube", "hi"));

        let got_any_result = tokio::time::timeout(Duration::from_millis(200), async {
            loop {
                match test_sub.recv().await {
                    Ok(e) if e.kind == "chat.send.failed" || e.kind == "chat.sent" => return true,
                    Ok(_) => continue,
                    Err(_) => return false,
                }
            }
        })
        .await;

        assert!(
            got_any_result.is_err(),
            "bridge must not emit any result for non-twitch targets"
        );
    }
}
