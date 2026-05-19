use forge_events::{Event, EventSource};
use forge_runtime::EventBus;
use forge_types::OAuthToken;
use serde::Deserialize;
use std::sync::Arc;
use thiserror::Error;

const EVENTSUB_URL: &str = "https://api.twitch.tv/helix/eventsub/subscriptions";
const EVENTSUB_PATH: &str = "/helix/eventsub/subscriptions";

#[derive(Debug, Error)]
pub(crate) enum SubscribeError {
    #[error("subscription HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("network error during subscription: {0}")]
    Network(String),
    #[error("scope missing; re-authentication required")]
    ScopeMissing,
}

#[derive(Debug, Deserialize)]
struct SubscribeResponse {
    data: Vec<SubscriptionData>,
}

#[derive(Debug, Deserialize)]
struct SubscriptionData {
    id: String,
    #[serde(rename = "type")]
    subscription_type: String,
    condition: serde_json::Value,
}

pub(crate) async fn subscribe_chat_message(
    token: &OAuthToken,
    client_id: &str,
    session_id: &str,
    broadcaster_id: &str,
    user_id: &str,
    bus: &Arc<EventBus>,
) -> Result<(), SubscribeError> {
    let http = reqwest::Client::new();
    let body = serde_json::json!({
        "type": "channel.chat.message",
        "version": "1",
        "condition": {
            "broadcaster_user_id": broadcaster_id,
            "user_id": user_id
        },
        "transport": {
            "method": "websocket",
            "session_id": session_id
        }
    });

    let resp = http
        .post(EVENTSUB_URL)
        .header("Authorization", format!("Bearer {}", token.expose()))
        .header("Client-Id", client_id)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| SubscribeError::Network(e.to_string()))?;

    let status = resp.status().as_u16();

    if status == 401 {
        bus.publish(Event::new(
            EventSource::Twitch,
            "request.fail",
            serde_json::json!({
                "endpoint": EVENTSUB_PATH,
                "status_code": status,
                "body_snippet": "unauthorized",
                "retry_after_secs": null,
            }),
        ));
        return Err(SubscribeError::ScopeMissing);
    }

    if !resp.status().is_success() {
        let retry_after = extract_retry_after(&resp);
        let body_text = resp.text().await.unwrap_or_default();
        let body_snippet: String = body_text.chars().take(200).collect();
        bus.publish(Event::new(
            EventSource::Twitch,
            "request.fail",
            serde_json::json!({
                "endpoint": EVENTSUB_PATH,
                "status_code": status,
                "body_snippet": body_snippet,
                "retry_after_secs": retry_after,
            }),
        ));
        return Err(SubscribeError::Http {
            status,
            body: body_text,
        });
    }

    let body_text = resp.text().await.unwrap_or_default();
    if let Ok(parsed) = serde_json::from_str::<SubscribeResponse>(&body_text)
        && let Some(sub) = parsed.data.first()
    {
        bus.publish(Event::new(
            EventSource::Twitch,
            "eventsub.subscription.created",
            serde_json::json!({
                "type": sub.subscription_type,
                "subscription_id": sub.id,
                "condition": sub.condition,
            }),
        ));
    }

    Ok(())
}

fn extract_retry_after(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_error_displays_non_empty() {
        let e = SubscribeError::Http {
            status: 400,
            body: "bad request".into(),
        };
        assert!(!e.to_string().is_empty());

        let e = SubscribeError::Network("timeout".into());
        assert!(!e.to_string().is_empty());

        let e = SubscribeError::ScopeMissing;
        assert!(!e.to_string().is_empty());
    }
}
