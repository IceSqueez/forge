use std::sync::Arc;

use forge_platform_core::PlatformError;
use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::quota_state::{QuotaState, today_pacific};

const DEFAULT_UPLOAD_BASE: &str = "https://www.googleapis.com/upload/youtube/v3";
const THUMBNAIL_SET_COST: u32 = 50;
const MAX_THUMBNAIL_BYTES: u64 = 2 * 1024 * 1024;

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubeThumbnail {
    client: reqwest::Client,
    access_token_source: TokenSource,
    active_broadcast_id: ActiveBroadcastIdHandle,
    quota: Arc<Mutex<QuotaState>>,
    api_base: String,
}

impl YoutubeThumbnail {
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
            api_base: DEFAULT_UPLOAD_BASE.to_owned(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    pub async fn set(&self, image_path: &str) -> Result<(), PlatformError> {
        let video_id =
            self.active_broadcast_id
                .get()
                .ok_or_else(|| PlatformError::Unsupported {
                    feature: "thumbnail - no active YouTube broadcast".to_owned(),
                })?;

        let bytes = tokio::fs::read(image_path).await?;
        if bytes.len() as u64 > MAX_THUMBNAIL_BYTES {
            return Err(PlatformError::Unsupported {
                feature: format!("thumbnail exceeds the {MAX_THUMBNAIL_BYTES}-byte API limit"),
            });
        }
        let content_type = content_type_for(image_path);

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(THUMBNAIL_SET_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let url = format!("{}/thumbnails/set", self.api_base);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .query(&[("videoId", video_id.as_str()), ("uploadType", "media")])
            .header("Content-Type", content_type)
            .body(bytes)
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
                    reason: "thumbnail upload scope missing".to_owned(),
                }
            }
            _ => PlatformError::Http {
                status,
                body: body_text,
            },
        }
    }
}

fn content_type_for(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    }
}
