use std::sync::Arc;

use forge_platform_core::PlatformError;
use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::quota_state::{QuotaState, today_pacific};

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
/// Undocumented for this endpoint; charged at the Data API's standard write-operation rate.
const CUEPOINT_COST: u32 = 50;

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubeAdBreak {
    client: reqwest::Client,
    access_token_source: TokenSource,
    active_broadcast_id: ActiveBroadcastIdHandle,
    quota: Arc<Mutex<QuotaState>>,
    api_base: String,
}

impl YoutubeAdBreak {
    pub fn new(
        access_token_source: TokenSource,
        active_broadcast_id: ActiveBroadcastIdHandle,
        quota: Arc<Mutex<QuotaState>>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token_source,
            active_broadcast_id,
            quota,
            api_base: DEFAULT_API_BASE.to_owned(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    /// The broadcast must be actively streaming for YouTube to accept the cuepoint.
    pub async fn insert_cuepoint(&self, duration_secs: u32) -> Result<(), PlatformError> {
        let broadcast_id =
            self.active_broadcast_id
                .get()
                .ok_or_else(|| PlatformError::Unsupported {
                    feature: "ad break - no active YouTube broadcast".to_owned(),
                })?;

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(CUEPOINT_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let url = format!("{}/liveBroadcasts/cuepoint", self.api_base);
        let payload = serde_json::json!({
            "cueType": "cueTypeAd",
            "durationSecs": duration_secs,
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .query(&[("id", broadcast_id.as_str())])
            .json(&payload)
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = resp.status().as_u16();
        if status == 200 || status == 201 {
            return Ok(());
        }
        Err(self.map_failure(resp).await)
    }

    async fn map_failure(&self, resp: reqwest::Response) -> PlatformError {
        let status = resp.status().as_u16();
        let retry_after_secs = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(30);
        let body_text = resp.text().await.unwrap_or_default();

        match status {
            429 => PlatformError::RateLimited { retry_after_secs },
            403 if body_text.contains("quotaExceeded") => PlatformError::QuotaExhausted,
            403 if body_text.contains("insufficientPermissions")
                || body_text.contains("operationNotSupported") =>
            {
                PlatformError::Auth {
                    reason: "ad break scope missing".to_owned(),
                }
            }
            _ => PlatformError::Http {
                status,
                body: body_text,
            },
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use crate::quota_state::QuotaState;

    const TOKEN_SENTINEL: &str = "yt-adbreak-secret-token";

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok(TOKEN_SENTINEL.to_owned()) }))
    }

    fn ad_break_on(
        server: &MockServer,
        broadcast: Option<&str>,
    ) -> (YoutubeAdBreak, Arc<Mutex<QuotaState>>) {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(broadcast.map(|s| s.to_owned()));
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let ad_break = YoutubeAdBreak::new(token_source(), handle, Arc::clone(&quota))
            .with_api_base(server.uri());
        (ad_break, quota)
    }

    #[tokio::test]
    async fn insert_returns_unsupported_when_no_active_broadcast() {
        let server = MockServer::start().await;
        let (ad_break, _quota) = ad_break_on(&server, None);

        let err = ad_break.insert_cuepoint(30).await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Unsupported { .. }),
            "expected Unsupported without a broadcast, got: {err}"
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn insert_posts_ad_cuepoint_with_id_query_and_charges_quota() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveBroadcasts/cuepoint"))
            .and(query_param("id", "bc-777"))
            .and(body_json(
                json!({ "cueType": "cueTypeAd", "durationSecs": 45 }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"kind": "x"})))
            .expect(1)
            .mount(&server)
            .await;

        let (ad_break, quota) = ad_break_on(&server, Some("bc-777"));

        ad_break.insert_cuepoint(45).await.unwrap();

        assert_eq!(
            quota.lock().await.used_today,
            CUEPOINT_COST,
            "successful cuepoint must charge {CUEPOINT_COST} units"
        );
    }

    #[tokio::test]
    async fn insert_maps_429_to_rate_limited_with_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveBroadcasts/cuepoint"))
            .respond_with(
                ResponseTemplate::new(429)
                    .append_header("retry-after", "45")
                    .set_body_string("slow down"),
            )
            .mount(&server)
            .await;

        let (ad_break, _quota) = ad_break_on(&server, Some("bc"));

        let err = ad_break.insert_cuepoint(30).await.unwrap_err();
        assert!(
            matches!(
                err,
                PlatformError::RateLimited {
                    retry_after_secs: 45
                }
            ),
            "expected RateLimited retry_after=45, got: {err}"
        );
    }

    #[tokio::test]
    async fn insert_error_does_not_leak_token_or_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveBroadcasts/cuepoint"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
            .mount(&server)
            .await;

        let (ad_break, _quota) = ad_break_on(&server, Some("bc"));

        let err = ad_break.insert_cuepoint(30).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.contains(TOKEN_SENTINEL),
            "error must not leak the bearer token: {msg}"
        );
        assert!(
            !msg.contains(&server.uri()),
            "error must not leak the request URL: {msg}"
        );
    }
}
