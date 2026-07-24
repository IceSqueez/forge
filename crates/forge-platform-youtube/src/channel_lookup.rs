use std::collections::BTreeMap;
use std::sync::Arc;

use forge_platform_core::PlatformError;
use forge_types::Variant;
use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::quota_state::{QuotaState, today_pacific};

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const LOOKUP_COST: u32 = 1;
const CHANNEL_ID_LEN: usize = 24;

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubeChannelLookup {
    client: reqwest::Client,
    access_token_source: TokenSource,
    quota: Arc<Mutex<QuotaState>>,
    api_base: String,
}

impl YoutubeChannelLookup {
    pub fn new(access_token_source: TokenSource, quota: Arc<Mutex<QuotaState>>) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token_source,
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

    /// `UC`-prefixed 24-char identifiers query by channel id; anything else queries by handle.
    pub async fn lookup(&self, identifier: &str) -> Result<Variant, PlatformError> {
        let is_channel_id = identifier.starts_with("UC") && identifier.len() == CHANNEL_ID_LEN;
        let filter_key = if is_channel_id { "id" } else { "forHandle" };

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(LOOKUP_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let url = format!("{}/channels", self.api_base);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .query(&[("part", "snippet,statistics"), (filter_key, identifier)])
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
            .ok_or_else(|| PlatformError::Http {
                status: 404,
                body: format!("channel not found: {identifier}"),
            })?;

        let channel_id = item.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        let title = item
            .pointer("/snippet/title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let subscriber_count = item
            .pointer("/statistics/subscriberCount")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let view_count = item
            .pointer("/statistics/viewCount")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        Ok(Variant::Object(BTreeMap::from([
            (
                "channel_id".to_owned(),
                Variant::String(channel_id.to_owned()),
            ),
            ("title".to_owned(), Variant::String(title.to_owned())),
            (
                "subscriber_count".to_owned(),
                Variant::Int(subscriber_count),
            ),
            ("view_count".to_owned(), Variant::Int(view_count)),
        ])))
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
                    reason: "channel lookup scope missing".to_owned(),
                }
            }
            _ => PlatformError::Http {
                status,
                body: body_text,
            },
        }
    }
}
