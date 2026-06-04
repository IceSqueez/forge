use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_events::EventPublisher;
use forge_platform_core::{BuiltinId, ConnectionState};
use forge_storage::{CredentialId, CredentialsRepo};
use tokio::sync::broadcast;

use crate::config::DiscordConfig;
use crate::content::{DiscordContentSnapshot, make_content_state, record_send};
use crate::credentials::{DISCORD_CRED_PREFIX, WebhookCredential};
use crate::embed::DiscordEmbed;
use crate::error::DiscordError;
use crate::events::{publish_failed, publish_posted, publish_ratelimit_hit};
use crate::health::{DiscordHealthSnapshot, make_health_state, update_on_send};
use crate::ratelimit::{DiscordRateLimiter, RateLimitOutcome};
use crate::sink::DiscordSink;

pub(crate) type HealthTx = broadcast::Sender<forge_platform_core::HealthDelta>;

pub struct DiscordClient {
    pub(crate) id: BuiltinId,
    #[allow(dead_code)]
    pub(crate) config: DiscordConfig,
    pub(crate) publisher: Arc<dyn EventPublisher>,
    pub(crate) creds: Arc<dyn CredentialsRepo>,
    pub(crate) http: reqwest::Client,
    pub(crate) health_tx: HealthTx,
    pub(crate) health_state: Arc<Mutex<DiscordHealthSnapshot>>,
    pub(crate) content_state: Arc<Mutex<DiscordContentSnapshot>>,
    pub(crate) rate_limiter: Arc<Mutex<DiscordRateLimiter>>,
}

impl DiscordClient {
    pub fn new(
        config: DiscordConfig,
        publisher: Arc<dyn EventPublisher>,
        creds: Arc<dyn CredentialsRepo>,
    ) -> Arc<Self> {
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()
            .unwrap_or_default();
        let (health_tx, health_state) = make_health_state();
        let content_state = make_content_state();
        let rate_limiter = Arc::new(Mutex::new(DiscordRateLimiter::new()));

        let client = Arc::new(Self {
            id: BuiltinId::new("discord"),
            config,
            publisher,
            creds,
            http,
            health_tx,
            health_state,
            content_state: Arc::clone(&content_state),
            rate_limiter,
        });

        let creds_ref = Arc::clone(&client.creds);
        let content_ref = Arc::clone(&content_state);
        tokio::spawn(async move {
            if let Ok(ids) = creds_ref.list_ids().await {
                let mut snap = content_ref.lock().unwrap_or_else(|p| p.into_inner());
                for id in ids {
                    if let Some(name) = id.as_str().strip_prefix(DISCORD_CRED_PREFIX)
                        && !snap.webhook_names.contains(&name.to_owned())
                    {
                        snap.webhook_names.push(name.to_owned());
                    }
                }
            }
        });

        client
    }

    pub(crate) fn connection_state(&self) -> ConnectionState {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        if snap.webhook_names.is_empty() {
            ConnectionState::Disconnected
        } else {
            ConnectionState::Connected
        }
    }

    async fn load_webhook(&self, name: &str) -> Result<WebhookCredential, DiscordError> {
        let id = CredentialId::new(format!("{DISCORD_CRED_PREFIX}{name}"));
        let json = self
            .creds
            .load(&id)
            .await
            .map_err(|e| DiscordError::Credential(e.to_string()))?
            .ok_or_else(|| DiscordError::WebhookNotFound {
                name: name.to_owned(),
            })?;
        let blob: serde_json::Value = serde_json::from_str(&json)?;
        let url = blob["url"]
            .as_str()
            .ok_or_else(|| DiscordError::Credential(format!("missing url in {name}")))?
            .to_owned();
        Ok(WebhookCredential {
            name: name.to_owned(),
            url,
        })
    }

    pub async fn post_text(
        &self,
        webhook_name: &str,
        content: &str,
    ) -> Result<String, DiscordError> {
        let cred = self.load_webhook(webhook_name).await?;
        let body = serde_json::json!({ "content": content });
        self.execute_post(&cred.url, body, webhook_name, 0).await
    }

    pub async fn post_embed(
        &self,
        webhook_name: &str,
        embed: DiscordEmbed,
    ) -> Result<String, DiscordError> {
        embed.validate()?;
        let cred = self.load_webhook(webhook_name).await?;
        let body = serde_json::json!({ "embeds": [embed_to_wire(&embed)] });
        self.execute_post(&cred.url, body, webhook_name, 1).await
    }

    pub async fn edit_message(
        &self,
        webhook_name: &str,
        message_id: &str,
        content: Option<&str>,
        embed: Option<DiscordEmbed>,
    ) -> Result<(), DiscordError> {
        if let Some(e) = &embed {
            e.validate()?;
        }
        let cred = self.load_webhook(webhook_name).await?;
        let edit_url = format!("{}/messages/{message_id}", cred.url);

        let mut map = serde_json::Map::new();
        if let Some(c) = content {
            map.insert(
                "content".to_owned(),
                serde_json::Value::String(c.to_owned()),
            );
        }
        if let Some(e) = &embed {
            map.insert("embeds".to_owned(), serde_json::json!([embed_to_wire(e)]));
        }
        let body = serde_json::Value::Object(map);
        self.execute_patch(&edit_url, body, webhook_name).await
    }

    async fn execute_post(
        &self,
        url: &str,
        body: serde_json::Value,
        webhook_name: &str,
        embed_count: u8,
    ) -> Result<String, DiscordError> {
        self.check_pre_send(webhook_name)?;
        let post_url = format!("{url}?wait=true");
        let start = std::time::Instant::now();
        let resp = self
            .http
            .post(&post_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| DiscordError::Connect(e.to_string()))?;

        let status = resp.status();
        if status.as_u16() == 429 {
            let retry_after = parse_retry_after(resp.headers());
            let is_global = is_global_rate_limit(resp.headers());
            let wait = Duration::from_secs_f64(retry_after.unwrap_or(1.0));

            self.record_429(webhook_name, wait, is_global);
            publish_ratelimit_hit(
                self.publisher.as_ref(),
                webhook_name,
                retry_after.unwrap_or(1.0),
            );

            tokio::time::sleep(wait).await;

            let retry_start = std::time::Instant::now();
            let retry_resp = self
                .http
                .post(&post_url)
                .json(&body)
                .send()
                .await
                .map_err(|e| DiscordError::Connect(e.to_string()))?;

            if retry_resp.status().as_u16() == 429 {
                let ra = parse_retry_after(retry_resp.headers()).unwrap_or(1.0);
                let err = DiscordError::RateLimited {
                    retry_after_secs: ra,
                };
                self.apply_send_result(
                    webhook_name,
                    retry_start.elapsed().as_millis() as u64,
                    false,
                    None,
                    embed_count,
                );
                publish_failed(self.publisher.as_ref(), webhook_name, &err.to_string());
                return Err(err);
            }

            let latency = retry_start.elapsed().as_millis() as u64;
            return self
                .handle_post_response(retry_resp, webhook_name, latency, embed_count)
                .await;
        }

        let latency = start.elapsed().as_millis() as u64;
        self.handle_post_response(resp, webhook_name, latency, embed_count)
            .await
    }

    async fn handle_post_response(
        &self,
        resp: reqwest::Response,
        webhook_name: &str,
        latency_ms: u64,
        embed_count: u8,
    ) -> Result<String, DiscordError> {
        let status = resp.status();
        let (rl_limit, rl_remaining, rl_reset) = parse_bucket_headers(resp.headers());
        self.update_bucket(webhook_name, rl_limit, rl_remaining, rl_reset);

        if status.is_success() {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let message_id = body["id"].as_str().unwrap_or("").to_owned();
            self.apply_send_result(
                webhook_name,
                latency_ms,
                true,
                Some(message_id.clone()),
                embed_count,
            );
            publish_posted(
                self.publisher.as_ref(),
                webhook_name,
                &message_id,
                embed_count,
            );
            Ok(message_id)
        } else {
            let code = status.as_u16();
            let body_text = resp.text().await.unwrap_or_default();
            let err = DiscordError::BadResponse {
                status: code,
                body: body_text.clone(),
            };
            self.apply_send_result(webhook_name, latency_ms, false, None, embed_count);
            publish_failed(self.publisher.as_ref(), webhook_name, &err.to_string());
            Err(err)
        }
    }

    async fn execute_patch(
        &self,
        url: &str,
        body: serde_json::Value,
        webhook_name: &str,
    ) -> Result<(), DiscordError> {
        self.check_pre_send(webhook_name)?;
        let start = std::time::Instant::now();
        let resp = self
            .http
            .patch(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| DiscordError::Connect(e.to_string()))?;

        let status = resp.status();
        if status.as_u16() == 429 {
            let retry_after = parse_retry_after(resp.headers());
            let is_global = is_global_rate_limit(resp.headers());
            let wait = Duration::from_secs_f64(retry_after.unwrap_or(1.0));

            self.record_429(webhook_name, wait, is_global);
            publish_ratelimit_hit(
                self.publisher.as_ref(),
                webhook_name,
                retry_after.unwrap_or(1.0),
            );

            tokio::time::sleep(wait).await;

            let retry_resp = self
                .http
                .patch(url)
                .json(&body)
                .send()
                .await
                .map_err(|e| DiscordError::Connect(e.to_string()))?;

            if retry_resp.status().as_u16() == 429 {
                let ra = parse_retry_after(retry_resp.headers()).unwrap_or(1.0);
                let err = DiscordError::RateLimited {
                    retry_after_secs: ra,
                };
                self.apply_send_result(
                    webhook_name,
                    start.elapsed().as_millis() as u64,
                    false,
                    None,
                    0,
                );
                publish_failed(self.publisher.as_ref(), webhook_name, &err.to_string());
                return Err(err);
            }

            return self
                .handle_patch_response(retry_resp, webhook_name, start.elapsed().as_millis() as u64)
                .await;
        }

        let latency = start.elapsed().as_millis() as u64;
        self.handle_patch_response(resp, webhook_name, latency)
            .await
    }

    async fn handle_patch_response(
        &self,
        resp: reqwest::Response,
        webhook_name: &str,
        latency_ms: u64,
    ) -> Result<(), DiscordError> {
        let status = resp.status();
        let (rl_limit, rl_remaining, rl_reset) = parse_bucket_headers(resp.headers());
        self.update_bucket(webhook_name, rl_limit, rl_remaining, rl_reset);

        if status.is_success() {
            self.apply_send_result(webhook_name, latency_ms, true, None, 0);
            Ok(())
        } else {
            let code = status.as_u16();
            let body_text = resp.text().await.unwrap_or_default();
            let err = DiscordError::BadResponse {
                status: code,
                body: body_text,
            };
            self.apply_send_result(webhook_name, latency_ms, false, None, 0);
            publish_failed(self.publisher.as_ref(), webhook_name, &err.to_string());
            Err(err)
        }
    }

    fn check_pre_send(&self, webhook_name: &str) -> Result<(), DiscordError> {
        let mut rl = self.rate_limiter.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(wait) = rl.global_wait_duration() {
            let ra = wait.as_secs_f64();
            return Err(DiscordError::RateLimited {
                retry_after_secs: ra,
            });
        }
        if let RateLimitOutcome::Throttled { wait_for } = rl.acquire(webhook_name) {
            let ra = wait_for.as_secs_f64();
            publish_ratelimit_hit(self.publisher.as_ref(), webhook_name, ra);
            return Err(DiscordError::RateLimited {
                retry_after_secs: ra,
            });
        }
        Ok(())
    }

    fn record_429(&self, webhook_name: &str, wait: Duration, is_global: bool) {
        let mut rl = self.rate_limiter.lock().unwrap_or_else(|p| p.into_inner());
        if is_global {
            rl.observe_global_throttle(wait);
        } else {
            rl.observe_remote_throttle(webhook_name, wait);
        }
    }

    fn update_bucket(&self, webhook_name: &str, limit: u32, remaining: u32, reset_after: f64) {
        if limit > 0 {
            let mut rl = self.rate_limiter.lock().unwrap_or_else(|p| p.into_inner());
            rl.record_response(webhook_name, limit, remaining, reset_after);
        }
    }

    fn apply_send_result(
        &self,
        webhook_name: &str,
        latency_ms: u64,
        ok: bool,
        message_id: Option<String>,
        embed_count: u8,
    ) {
        let (rl_remaining, rl_total) = {
            let rl = self.rate_limiter.lock().unwrap_or_else(|p| p.into_inner());
            rl.budget(webhook_name)
        };
        let reset_hint = {
            let rl = self.rate_limiter.lock().unwrap_or_else(|p| p.into_inner());
            rl.reset_hint_secs(webhook_name).map(|s| format!("{s:.0}"))
        };

        let deltas = {
            let mut snap = self.health_state.lock().unwrap_or_else(|p| p.into_inner());
            update_on_send(
                &mut snap,
                latency_ms,
                ok,
                rl_remaining,
                rl_total,
                reset_hint,
            )
        };
        for delta in deltas {
            let _ = self.health_tx.send(delta);
        }

        {
            let mut snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
            record_send(&mut snap, webhook_name, message_id, embed_count > 0, ok);
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test() -> Arc<Self> {
        use forge_storage::{CredentialId, StorageError};
        use time::OffsetDateTime;

        struct NoopPublisher;
        impl EventPublisher for NoopPublisher {
            fn publish(&self, _: forge_events::Event) {}
        }

        struct EmptyCreds;
        #[async_trait::async_trait]
        impl CredentialsRepo for EmptyCreds {
            async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
                Ok(())
            }
            async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
                Ok(None)
            }
            async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
                Ok(false)
            }
            async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
                Ok(vec![])
            }
            async fn last_refresh(
                &self,
                _: &CredentialId,
            ) -> Result<Option<OffsetDateTime>, StorageError> {
                Ok(None)
            }
            async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
                Ok(())
            }
        }

        let (health_tx, health_state) = make_health_state();
        let content_state = make_content_state();
        let http = reqwest::Client::new();
        Arc::new(Self {
            id: BuiltinId::new("discord"),
            config: DiscordConfig::default(),
            publisher: Arc::new(NoopPublisher),
            creds: Arc::new(EmptyCreds),
            http,
            health_tx,
            health_state,
            content_state,
            rate_limiter: Arc::new(Mutex::new(DiscordRateLimiter::new())),
        })
    }
}

#[async_trait::async_trait]
impl DiscordSink for DiscordClient {
    async fn post_text(&self, webhook_name: &str, content: &str) -> Result<String, DiscordError> {
        self.post_text(webhook_name, content).await
    }

    async fn post_embed(
        &self,
        webhook_name: &str,
        embed: DiscordEmbed,
    ) -> Result<String, DiscordError> {
        self.post_embed(webhook_name, embed).await
    }

    async fn edit_message(
        &self,
        webhook_name: &str,
        message_id: &str,
        content: Option<&str>,
        embed: Option<DiscordEmbed>,
    ) -> Result<(), DiscordError> {
        self.edit_message(webhook_name, message_id, content, embed)
            .await
    }
}

fn embed_to_wire(embed: &DiscordEmbed) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    if let Some(t) = &embed.title {
        m.insert("title".to_owned(), t.clone().into());
    }
    if let Some(d) = &embed.description {
        m.insert("description".to_owned(), d.clone().into());
    }
    if let Some(c) = embed.color {
        m.insert("color".to_owned(), serde_json::json!(c));
    }
    if !embed.fields.is_empty() {
        let fields: Vec<serde_json::Value> = embed
            .fields
            .iter()
            .map(|f| {
                serde_json::json!({
                    "name":   f.name,
                    "value":  f.value,
                    "inline": f.inline
                })
            })
            .collect();
        m.insert("fields".to_owned(), serde_json::Value::Array(fields));
    }
    if let Some(url) = &embed.thumbnail_url {
        m.insert("thumbnail".to_owned(), serde_json::json!({ "url": url }));
    }
    if let Some(url) = &embed.image_url {
        m.insert("image".to_owned(), serde_json::json!({ "url": url }));
    }
    if let Some(text) = &embed.footer_text {
        m.insert("footer".to_owned(), serde_json::json!({ "text": text }));
    }
    if let Some(name) = &embed.author_name {
        m.insert("author".to_owned(), serde_json::json!({ "name": name }));
    }
    if let Some(ts) = &embed.timestamp {
        let ts_str = ts
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        m.insert("timestamp".to_owned(), ts_str.into());
    }
    serde_json::Value::Object(m)
}

fn parse_bucket_headers(headers: &reqwest::header::HeaderMap) -> (u32, u32, f64) {
    let limit = header_u32(headers, "x-ratelimit-limit").unwrap_or(0);
    let remaining = header_u32(headers, "x-ratelimit-remaining").unwrap_or(0);
    let reset_after = header_f64(headers, "x-ratelimit-reset-after").unwrap_or(0.0);
    (limit, remaining, reset_after)
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<f64> {
    header_f64(headers, "retry-after")
}

fn is_global_rate_limit(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get("x-ratelimit-global")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

fn header_u32(headers: &reqwest::header::HeaderMap, key: &str) -> Option<u32> {
    headers.get(key)?.to_str().ok()?.parse().ok()
}

fn header_f64(headers: &reqwest::header::HeaderMap, key: &str) -> Option<f64> {
    headers.get(key)?.to_str().ok()?.parse().ok()
}
