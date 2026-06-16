use std::sync::Arc;

use forge_platform_core::PlatformError;
use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::quota_state::{QuotaState, today_pacific};

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const FETCH_COST: u32 = 1;
const UPDATE_COST: u32 = 50;

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

enum Field {
    Title,
    Description,
    Category,
    Privacy,
}

pub struct YoutubeStreamMetadata {
    client: reqwest::Client,
    access_token_source: TokenSource,
    active_broadcast_id: ActiveBroadcastIdHandle,
    quota: Arc<Mutex<QuotaState>>,
    api_base: String,
}

impl YoutubeStreamMetadata {
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

    pub async fn set_title(&self, value: &str) -> Result<(), PlatformError> {
        self.update(Field::Title, value).await
    }

    pub async fn set_description(&self, value: &str) -> Result<(), PlatformError> {
        self.update(Field::Description, value).await
    }

    pub async fn set_category(&self, value: &str) -> Result<(), PlatformError> {
        self.update(Field::Category, value).await
    }

    pub async fn set_privacy(&self, value: &str) -> Result<(), PlatformError> {
        self.update(Field::Privacy, value).await
    }

    /// Fetch-merge-write: `videos.update` clears any `part` field omitted from the
    /// request body, so the current `snippet`+`status` is fetched, the single target
    /// field merged in, and the full merged resource written back.
    async fn update(&self, field: Field, value: &str) -> Result<(), PlatformError> {
        let broadcast_id =
            self.active_broadcast_id
                .get()
                .ok_or_else(|| PlatformError::Unsupported {
                    feature: "stream metadata — no active YouTube broadcast".to_owned(),
                })?;

        let url = format!("{}/videos", self.api_base);

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(FETCH_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .query(&[("part", "snippet,status"), ("id", broadcast_id.as_str())])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = resp.status().as_u16();
        if status != 200 {
            return Err(self.map_failure(resp).await);
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| PlatformError::Network {
            reason: e.without_url().to_string(),
        })?;

        let item = body
            .get("items")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "stream metadata — active broadcast video not found".to_owned(),
            })?;

        let mut snippet = item
            .get("snippet")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let mut status_part = item
            .get("status")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        match field {
            Field::Title => snippet["title"] = serde_json::json!(value),
            Field::Description => snippet["description"] = serde_json::json!(value),
            Field::Category => snippet["categoryId"] = serde_json::json!(value),
            Field::Privacy => status_part["privacyStatus"] = serde_json::json!(value),
        }

        let merged = serde_json::json!({
            "id": broadcast_id,
            "snippet": snippet,
            "status": status_part,
        });

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(UPDATE_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let resp = self
            .client
            .put(&url)
            .bearer_auth(&token)
            .query(&[("part", "snippet,status")])
            .json(&merged)
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
                    reason: "stream metadata scope missing".to_owned(),
                }
            }
            _ => PlatformError::Http {
                status,
                body: body_text,
            },
        }
    }
}
