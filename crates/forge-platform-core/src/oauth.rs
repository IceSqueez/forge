use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use forge_types::{OAuthToken, RefreshToken};
use serde::Deserialize;

use crate::error::PlatformError;

#[derive(Debug, Clone)]
pub struct DeviceCodeRequest {
    pub client_id: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    #[serde(deserialize_with = "de_secs_as_duration")]
    pub expires_in: Duration,
    #[serde(deserialize_with = "de_secs_as_duration")]
    pub interval: Duration,
}

#[derive(Clone)]
pub struct TokenResponse {
    pub access_token: OAuthToken,
    pub refresh_token: Option<RefreshToken>,
    pub expires_in: Duration,
    pub scopes: Vec<String>,
}

impl std::fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenResponse")
            .field("access_token", &self.access_token)
            .field("refresh_token", &self.refresh_token)
            .field("expires_in", &self.expires_in)
            .field("scopes", &self.scopes)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum PollOutcome {
    Pending,
    SlowDown,
    Success(TokenResponse),
    Expired,
    Denied,
    Cancelled,
}

pub struct DeviceCodePoller {
    client_id: String,
    token_endpoint: String,
    device_code: String,
    interval: Duration,
    expires_at: Instant,
    http: reqwest::Client,
    cancel: Arc<AtomicBool>,
}

impl DeviceCodePoller {
    pub async fn request_device_code(
        user_code_endpoint: &str,
        req: DeviceCodeRequest,
    ) -> Result<DeviceCodeResponse, PlatformError> {
        let http = reqwest::Client::new();
        let scopes = req.scopes.join(" ");
        let resp = http
            .post(user_code_endpoint)
            .form(&[("client_id", &*req.client_id), ("scopes", &*scopes)])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.to_string(),
            })?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(PlatformError::Http { status, body });
        }

        resp.json::<DeviceCodeResponse>()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.to_string(),
            })
    }

    pub fn new(
        client_id: impl Into<String>,
        token_endpoint: impl Into<String>,
        device_code: impl Into<String>,
        interval: Duration,
        expires_in: Duration,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            token_endpoint: token_endpoint.into(),
            device_code: device_code.into(),
            interval,
            expires_at: Instant::now() + expires_in,
            http: reqwest::Client::new(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn poll_once(&self) -> Result<PollOutcome, PlatformError> {
        let resp = self
            .http
            .post(&self.token_endpoint)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", &self.client_id),
                ("device_code", &self.device_code),
            ])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.to_string(),
            })?;

        if resp.status().is_success() {
            let body: TokenEndpointSuccess =
                resp.json().await.map_err(|e| PlatformError::Network {
                    reason: e.to_string(),
                })?;
            let scopes = body
                .scope
                .as_deref()
                .unwrap_or("")
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            return Ok(PollOutcome::Success(TokenResponse {
                access_token: OAuthToken::new(body.access_token),
                refresh_token: body.refresh_token.map(RefreshToken::new),
                expires_in: Duration::from_secs(body.expires_in),
                scopes,
            }));
        }

        let error_body: TokenEndpointError =
            resp.json().await.map_err(|e| PlatformError::Network {
                reason: e.to_string(),
            })?;

        Ok(error_field_to_outcome(&error_body.error))
    }

    pub async fn run(&mut self) -> Result<TokenResponse, PlatformError> {
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                return Err(PlatformError::Auth {
                    reason: "cancelled".into(),
                });
            }

            tokio::time::sleep(self.interval).await;

            if Instant::now() >= self.expires_at {
                return Err(PlatformError::Auth {
                    reason: "device code expired".into(),
                });
            }

            if self.cancel.load(Ordering::Relaxed) {
                return Err(PlatformError::Auth {
                    reason: "cancelled".into(),
                });
            }

            match self.poll_once().await? {
                PollOutcome::Pending => {}
                PollOutcome::SlowDown => {
                    self.interval += Duration::from_secs(5);
                }
                PollOutcome::Success(token) => return Ok(token),
                PollOutcome::Expired => {
                    return Err(PlatformError::Auth {
                        reason: "device code expired".into(),
                    });
                }
                PollOutcome::Denied => {
                    return Err(PlatformError::Auth {
                        reason: "user denied".into(),
                    });
                }
                PollOutcome::Cancelled => {
                    return Err(PlatformError::Auth {
                        reason: "cancelled".into(),
                    });
                }
            }
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn cancel_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }
}

fn error_field_to_outcome(error: &str) -> PollOutcome {
    match error {
        "authorization_pending" => PollOutcome::Pending,
        "slow_down" => PollOutcome::SlowDown,
        "expired_token" => PollOutcome::Expired,
        "access_denied" => PollOutcome::Denied,
        _ => PollOutcome::Expired,
    }
}

fn de_secs_as_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let secs = u64::deserialize(deserializer)?;
    Ok(Duration::from_secs(secs))
}

#[derive(Debug, Deserialize)]
struct TokenEndpointSuccess {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenEndpointError {
    error: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn device_code_response_deserialize() {
        let json = r#"{
            "device_code": "NGExNWIzYjItN2FjZi00ODFkLTkxMTYt",
            "user_code": "WDJB-MJHT",
            "verification_uri": "https://www.twitch.tv/activate",
            "verification_uri_complete": "https://www.twitch.tv/activate?device-code=WDJB-MJHT",
            "expires_in": 1800,
            "interval": 5
        }"#;

        let resp: DeviceCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.device_code, "NGExNWIzYjItN2FjZi00ODFkLTkxMTYt");
        assert_eq!(resp.user_code, "WDJB-MJHT");
        assert_eq!(resp.verification_uri, "https://www.twitch.tv/activate");
        assert_eq!(
            resp.verification_uri_complete.as_deref(),
            Some("https://www.twitch.tv/activate?device-code=WDJB-MJHT")
        );
        assert_eq!(resp.expires_in, Duration::from_secs(1800));
        assert_eq!(resp.interval, Duration::from_secs(5));
    }

    #[test]
    fn poll_outcome_pending_from_error_field() {
        assert!(matches!(
            error_field_to_outcome("authorization_pending"),
            PollOutcome::Pending
        ));
        assert!(matches!(
            error_field_to_outcome("slow_down"),
            PollOutcome::SlowDown
        ));
        assert!(matches!(
            error_field_to_outcome("expired_token"),
            PollOutcome::Expired
        ));
        assert!(matches!(
            error_field_to_outcome("access_denied"),
            PollOutcome::Denied
        ));
        assert!(matches!(
            error_field_to_outcome("unknown_error"),
            PollOutcome::Expired
        ));
    }

    #[test]
    fn poller_cancellation_via_flag() {
        let poller = DeviceCodePoller::new(
            "client_id",
            "https://id.twitch.tv/oauth2/token",
            "device_code_abc",
            Duration::from_secs(5),
            Duration::from_secs(1800),
        );

        assert!(!poller.cancel.load(Ordering::Relaxed));
        poller.cancel();
        assert!(poller.cancel.load(Ordering::Relaxed));

        let token = poller.cancel_token();
        assert!(token.load(Ordering::Relaxed));
    }
}
