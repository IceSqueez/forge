use forge_types::OAuthToken;
use thiserror::Error;

const EVENTSUB_URL: &str = "https://api.twitch.tv/helix/eventsub/subscriptions";

#[derive(Debug, Error)]
pub(crate) enum SubscribeError {
    #[error("subscription HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("network error during subscription: {0}")]
    Network(String),
    #[error("scope missing; re-authentication required")]
    ScopeMissing,
}

pub(crate) async fn subscribe_chat_message(
    http: &reqwest::Client,
    token: &OAuthToken,
    client_id: &str,
    session_id: &str,
    broadcaster_id: &str,
    user_id: &str,
) -> Result<(), SubscribeError> {
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
        return Err(SubscribeError::ScopeMissing);
    }

    if !resp.status().is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        return Err(SubscribeError::Http {
            status,
            body: body_text,
        });
    }

    Ok(())
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
