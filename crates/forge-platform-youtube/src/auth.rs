use std::time::{Duration, SystemTime};

use forge_platform_core::AuthFlow;
use forge_types::OAuthToken;
use thiserror::Error;

pub const GOOGLE_DEVICE_ENDPOINT: &str = "https://oauth2.googleapis.com/device/code";
pub const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

const YOUTUBE_CHANNELS_ENDPOINT: &str = "https://www.googleapis.com/youtube/v3/channels";

pub const YOUTUBE_BROADCASTER_SCOPES: &[&str] =
    &["https://www.googleapis.com/auth/youtube.force-ssl"];

pub fn youtube_auth_flow() -> AuthFlow {
    AuthFlow::DeviceCode {
        user_code_endpoint: GOOGLE_DEVICE_ENDPOINT.to_owned(),
        token_endpoint: GOOGLE_TOKEN_ENDPOINT.to_owned(),
        scopes: YOUTUBE_BROADCASTER_SCOPES
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
    }
}

#[derive(Debug, Clone)]
pub struct YoutubeDeviceCode {
    pub user_code: String,
    pub verification_url: String,
    pub expires_in: Duration,
    pub interval: Duration,
    pub device_code: String,
}

#[derive(Debug, Clone)]
pub struct YoutubeAuthBundle {
    pub access_token: OAuthToken,
    pub refresh_token: OAuthToken,
    pub channel_id: String,
    pub channel_title: String,
    pub client_id: String,
    pub expires_at: SystemTime,
}

pub struct GoogleAuthFlow {
    client: reqwest::Client,
    client_id: String,
    client_secret: String,
    device_endpoint: String,
    token_endpoint: String,
    channels_endpoint: String,
}

impl GoogleAuthFlow {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self::with_endpoints(
            client_id,
            client_secret,
            GOOGLE_DEVICE_ENDPOINT.to_owned(),
            GOOGLE_TOKEN_ENDPOINT.to_owned(),
            YOUTUBE_CHANNELS_ENDPOINT.to_owned(),
        )
    }

    pub(crate) fn with_endpoints(
        client_id: String,
        client_secret: String,
        device_endpoint: String,
        token_endpoint: String,
        channels_endpoint: String,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            client_id,
            client_secret,
            device_endpoint,
            token_endpoint,
            channels_endpoint,
        }
    }

    pub(crate) fn client_secret(&self) -> &str {
        &self.client_secret
    }

    pub(crate) fn token_endpoint(&self) -> &str {
        &self.token_endpoint
    }

    pub(crate) fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    pub async fn start(&self) -> Result<YoutubeDeviceCode, YoutubeAuthError> {
        let scope_string = YOUTUBE_BROADCASTER_SCOPES.join(" ");
        let response = self
            .client
            .post(&self.device_endpoint)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scope", scope_string.as_str()),
            ])
            .send()
            .await
            .map_err(|e| YoutubeAuthError::Network(e.to_string()))?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(YoutubeAuthError::Http { status, body });
        }

        let parsed: DeviceCodeResponse = response
            .json()
            .await
            .map_err(|e| YoutubeAuthError::Network(e.to_string()))?;

        Ok(YoutubeDeviceCode {
            user_code: parsed.user_code,
            verification_url: parsed.verification_url,
            expires_in: Duration::from_secs(parsed.expires_in),
            interval: Duration::from_secs(parsed.interval),
            device_code: parsed.device_code,
        })
    }

    pub async fn wait_for_authorization(
        &self,
        device_code: &str,
        interval: Duration,
    ) -> Result<YoutubeAuthBundle, YoutubeAuthError> {
        let mut current_interval = interval;

        loop {
            tokio::time::sleep(current_interval).await;

            let response = self
                .client
                .post(&self.token_endpoint)
                .form(&[
                    ("client_id", self.client_id.as_str()),
                    ("client_secret", self.client_secret.as_str()),
                    ("device_code", device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ])
                .send()
                .await
                .map_err(|e| YoutubeAuthError::Network(e.to_string()))?;

            let status = response.status().as_u16();

            if status == 200 {
                let token: TokenSuccessResponse = response
                    .json()
                    .await
                    .map_err(|e| YoutubeAuthError::Network(e.to_string()))?;
                return self.resolve_channel(token).await;
            }

            let body = response.text().await.unwrap_or_default();
            let parsed_error: Result<TokenErrorResponse, _> = serde_json::from_str(&body);

            match parsed_error.as_ref().map(|e| e.error.as_str()) {
                Ok("authorization_pending") => {}
                Ok("slow_down") => current_interval += Duration::from_secs(5),
                Ok("access_denied") => return Err(YoutubeAuthError::AccessDenied),
                Ok("expired_token") => return Err(YoutubeAuthError::ExpiredToken),
                _ => return Err(YoutubeAuthError::Http { status, body }),
            }
        }
    }

    async fn resolve_channel(
        &self,
        token: TokenSuccessResponse,
    ) -> Result<YoutubeAuthBundle, YoutubeAuthError> {
        let response = self
            .client
            .get(&self.channels_endpoint)
            .query(&[("part", "snippet"), ("mine", "true")])
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", token.access_token),
            )
            .send()
            .await
            .map_err(|e| YoutubeAuthError::Network(e.to_string()))?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(YoutubeAuthError::Http { status, body });
        }

        let channels: ChannelsListResponse = response
            .json()
            .await
            .map_err(|e| YoutubeAuthError::Network(e.to_string()))?;

        let channel = channels
            .items
            .into_iter()
            .next()
            .ok_or_else(|| YoutubeAuthError::Http {
                status: 200,
                body: "no channel found".to_owned(),
            })?;

        Ok(YoutubeAuthBundle {
            access_token: OAuthToken::new(token.access_token),
            refresh_token: OAuthToken::new(token.refresh_token),
            channel_id: channel.id,
            channel_title: channel.snippet.title,
            client_id: self.client_id.clone(),
            expires_at: SystemTime::now() + Duration::from_secs(token.expires_in),
        })
    }
}

#[derive(serde::Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_url: String,
    expires_in: u64,
    interval: u64,
}

#[derive(serde::Deserialize)]
struct TokenSuccessResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

#[derive(serde::Deserialize)]
struct TokenErrorResponse {
    error: String,
}

#[derive(serde::Deserialize)]
struct ChannelsListResponse {
    items: Vec<ChannelItem>,
}

#[derive(serde::Deserialize)]
struct ChannelItem {
    id: String,
    snippet: ChannelSnippet,
}

#[derive(serde::Deserialize)]
struct ChannelSnippet {
    title: String,
}

#[derive(Debug, Error)]
pub enum YoutubeAuthError {
    #[error("HTTP error {status}: {body}")]
    Http { status: u16, body: String },
    #[error("network error: {0}")]
    Network(String),
    #[error("access denied by user")]
    AccessDenied,
    #[error("device code or token has expired")]
    ExpiredToken,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_platform_core::AuthFlow;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn token_success_body() -> serde_json::Value {
        json!({
            "access_token": "ya29.test_access",
            "refresh_token": "1//test_refresh",
            "expires_in": 3599,
            "token_type": "Bearer",
            "scope": "https://www.googleapis.com/auth/youtube.force-ssl"
        })
    }

    fn channel_body() -> serde_json::Value {
        json!({
            "items": [
                {
                    "id": "UCtest123",
                    "snippet": { "title": "Test Channel" }
                }
            ]
        })
    }

    async fn mount_channel_mock(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(channel_body()))
            .mount(server)
            .await;
    }

    #[test]
    fn youtube_auth_flow_returns_device_code_variant() {
        let flow = youtube_auth_flow();
        let AuthFlow::DeviceCode {
            user_code_endpoint,
            token_endpoint,
            scopes,
        } = flow
        else {
            unreachable!("youtube_auth_flow must return DeviceCode variant");
        };
        assert_eq!(user_code_endpoint, GOOGLE_DEVICE_ENDPOINT);
        assert_eq!(token_endpoint, GOOGLE_TOKEN_ENDPOINT);
        assert_eq!(
            scopes,
            YOUTUBE_BROADCASTER_SCOPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn scope_list_contains_force_ssl() {
        assert!(
            YOUTUBE_BROADCASTER_SCOPES
                .iter()
                .any(|s| s.contains("youtube.force-ssl"))
        );
    }

    #[tokio::test]
    async fn start_parses_device_code_response() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/device/code"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "device_code": "dev_code_abc",
                "user_code": "ABCD-1234",
                "verification_url": "https://google.com/device",
                "expires_in": 1800,
                "interval": 5
            })))
            .mount(&server)
            .await;

        let flow = GoogleAuthFlow::with_endpoints(
            "test_client".to_owned(),
            "test_secret".to_owned(),
            format!("{}/device/code", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/channels", server.uri()),
        );

        let result = flow.start().await.unwrap();
        assert_eq!(result.user_code, "ABCD-1234");
        assert_eq!(result.verification_url, "https://google.com/device");
        assert_eq!(result.expires_in, Duration::from_secs(1800));
        assert_eq!(result.interval, Duration::from_secs(5));
        assert_eq!(result.device_code, "dev_code_abc");
    }

    #[tokio::test]
    async fn start_returns_http_error_on_500() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/device/code"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .mount(&server)
            .await;

        let flow = GoogleAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            format!("{}/device/code", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/channels", server.uri()),
        );

        let result = flow.start().await;
        assert!(matches!(
            result,
            Err(YoutubeAuthError::Http { status: 500, .. })
        ));
    }

    #[tokio::test]
    async fn wait_handles_authorization_pending_then_succeeds() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"error": "authorization_pending"})),
            )
            .up_to_n_times(2)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_success_body()))
            .mount(&server)
            .await;

        mount_channel_mock(&server).await;

        let flow = GoogleAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            format!("{}/device/code", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/channels", server.uri()),
        );

        let interval = Duration::from_millis(50);
        let start = std::time::Instant::now();
        let result = flow.wait_for_authorization("dc123", interval).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        let bundle = result.unwrap();
        assert_eq!(bundle.channel_id, "UCtest123");
        assert_eq!(bundle.channel_title, "Test Channel");
        assert_eq!(bundle.client_id, "cid");
        assert!(
            elapsed >= Duration::from_millis(100),
            "expected elapsed >= 100ms (2 × interval), got {elapsed:?}",
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wait_handles_slow_down_increments_interval() {
        tokio::time::pause();

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error": "slow_down"})))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_success_body()))
            .mount(&server)
            .await;

        mount_channel_mock(&server).await;

        let flow = GoogleAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            format!("{}/device/code", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/channels", server.uri()),
        );

        let initial_interval = Duration::from_secs(2);
        let handle = tokio::spawn(async move {
            flow.wait_for_authorization("dc_slow", initial_interval)
                .await
        });

        // Advance past the initial 2s sleep so the first token poll fires.
        tokio::time::advance(Duration::from_secs(2) + Duration::from_millis(50)).await;
        // Yield until the HTTP round-trip completes and the task re-enters sleep.
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }

        // After slow_down the interval is 2s + 5s = 7s. Advancing only 6s must not
        // be enough to wake the task for its second poll.
        tokio::time::advance(Duration::from_secs(6)).await;
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert!(
            !handle.is_finished(),
            "task should still be sleeping: interval was incremented to 7s, only 6s elapsed",
        );

        // Advance the remaining second plus a margin — task wakes, polls success,
        // resolves channel, returns Ok.
        tokio::time::advance(Duration::from_secs(1) + Duration::from_millis(50)).await;
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }

        let result = handle.await.unwrap();
        assert!(
            result.is_ok(),
            "expected Ok after slow_down + success, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn wait_returns_access_denied() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"error": "access_denied"})),
            )
            .mount(&server)
            .await;

        let flow = GoogleAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            format!("{}/device/code", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/channels", server.uri()),
        );

        let result = flow
            .wait_for_authorization("dc", Duration::from_millis(1))
            .await;
        assert!(
            matches!(result, Err(YoutubeAuthError::AccessDenied)),
            "expected AccessDenied, got: {result:?}",
        );
    }

    #[tokio::test]
    async fn wait_returns_expired_token() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"error": "expired_token"})),
            )
            .mount(&server)
            .await;

        let flow = GoogleAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            format!("{}/device/code", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/channels", server.uri()),
        );

        let result = flow
            .wait_for_authorization("dc", Duration::from_millis(1))
            .await;
        assert!(
            matches!(result, Err(YoutubeAuthError::ExpiredToken)),
            "expected ExpiredToken, got: {result:?}",
        );
    }

    #[tokio::test]
    async fn wait_returns_error_for_empty_channels_response() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_success_body()))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
            .mount(&server)
            .await;

        let flow = GoogleAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            format!("{}/device/code", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/channels", server.uri()),
        );

        let result = flow
            .wait_for_authorization("dc", Duration::from_millis(1))
            .await;
        assert!(
            matches!(result, Err(YoutubeAuthError::Http { status: 200, .. })),
            "expected Http error for empty items, got: {result:?}",
        );
    }
}
