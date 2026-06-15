use std::collections::HashMap;
use std::sync::Arc;

use forge_platform_core::PlatformError;
use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::live_chat_id::LiveChatIdHandle;
use crate::quota_state::{QuotaState, today_pacific};

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const BAN_COST: u32 = 50;

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubeModeration {
    client: reqwest::Client,
    access_token_source: TokenSource,
    live_chat_id: LiveChatIdHandle,
    quota: Arc<Mutex<QuotaState>>,
    /// Maps a banned channel id to the ban resource id returned by `insert`, so a
    /// later unban can target the resource YouTube's API exposes no lookup for.
    ban_ids: Mutex<HashMap<String, String>>,
    api_base: String,
}

impl YoutubeModeration {
    pub fn new(
        access_token_source: TokenSource,
        live_chat_id: LiveChatIdHandle,
        quota: Arc<Mutex<QuotaState>>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token_source,
            live_chat_id,
            quota,
            ban_ids: Mutex::new(HashMap::new()),
            api_base: DEFAULT_API_BASE.to_owned(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    pub async fn ban(&self, channel_id: &str) -> Result<(), PlatformError> {
        let ban_id = self.insert_ban(channel_id, None).await?;
        self.ban_ids
            .lock()
            .await
            .insert(channel_id.to_owned(), ban_id);
        Ok(())
    }

    pub async fn timeout(
        &self,
        channel_id: &str,
        duration_seconds: u32,
    ) -> Result<(), PlatformError> {
        let ban_id = self.insert_ban(channel_id, Some(duration_seconds)).await?;
        self.ban_ids
            .lock()
            .await
            .insert(channel_id.to_owned(), ban_id);
        Ok(())
    }

    pub async fn unban(&self, channel_id: &str) -> Result<(), PlatformError> {
        let ban_id = self
            .ban_ids
            .lock()
            .await
            .get(channel_id)
            .cloned()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "unban — ban id unknown for this channel (ban not issued in this session)"
                    .to_owned(),
            })?;

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(BAN_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let url = format!("{}/liveChat/bans", self.api_base);

        let resp = self
            .client
            .delete(&url)
            .bearer_auth(&token)
            .query(&[("id", ban_id.as_str())])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = resp.status().as_u16();
        if status == 200 || status == 204 {
            self.ban_ids.lock().await.remove(channel_id);
            return Ok(());
        }
        Err(self.map_failure(resp).await)
    }

    async fn insert_ban(
        &self,
        channel_id: &str,
        duration_seconds: Option<u32>,
    ) -> Result<String, PlatformError> {
        let live_chat_id = self
            .live_chat_id
            .get()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "moderation — no active YouTube broadcast".to_owned(),
            })?;

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(BAN_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let url = format!("{}/liveChat/bans", self.api_base);

        let mut ban_details = serde_json::json!({
            "liveChatId": live_chat_id,
            "type": if duration_seconds.is_some() { "temporary" } else { "permanent" },
            "bannedUserDetails": { "channelId": channel_id },
        });
        if let Some(secs) = duration_seconds {
            ban_details["banDurationSeconds"] = serde_json::json!(secs.to_string());
        }
        let payload = serde_json::json!({ "snippet": ban_details });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .query(&[("part", "snippet")])
            .json(&payload)
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = resp.status().as_u16();
        if status == 200 || status == 201 {
            let body: serde_json::Value =
                resp.json().await.map_err(|e| PlatformError::Network {
                    reason: e.without_url().to_string(),
                })?;
            let ban_id = body
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned();
            return Ok(ban_id);
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
                    reason: "moderation scope missing".to_owned(),
                }
            }
            _ => PlatformError::Http {
                status,
                body: body_text,
            },
        }
    }
}
