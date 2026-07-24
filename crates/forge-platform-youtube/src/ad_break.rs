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
