use forge_types::{IntegrationId, RecoverMutex, Secret};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_events::EventPublisher;
use forge_net::{
    CallClass, Gateway, GatewayFailure, GatewayProfile, GatewayRequest, GatewayResponse,
    GatewayTimings, HeaderLookup, Method, MultipartBody, RequestBody, ThrottleHintFamily,
    ThrottleScope,
};
use forge_platform_core::CollectionRevisionSignal;
use forge_storage::CredentialsRepo;
use tokio::sync::broadcast;

use crate::content::{DiscordContentSnapshot, make_content_state, record_send};
use crate::credentials::WebhookCredential;
use crate::embed::{DiscordEmbed, MAX_CONTENT_CHARS};
use crate::error::{DiscordError, ValidationFault};
use crate::events::{publish_failed, publish_posted, publish_rate_limited};
use crate::health::{
    DiscordHealthSnapshot, make_health_state, record_unsent_failure, update_on_send,
};
use crate::mention::MentionPolicy;
use crate::ratelimit::{
    DiscordRateLimiter, GlobalThrottleGate, RateLimitOutcome, SharedDiscordRateLimiter,
};
use crate::route::WebhookRoute;
use crate::sink::DiscordSink;

pub(crate) type HealthTx = broadcast::Sender<forge_platform_core::HealthDelta>;

const NO_HINT_RETRY_WAIT: Duration = Duration::from_secs(1);

const DISCORD_GATEWAY_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::Interactive,
    ThrottleHintFamily::ResetAfterSeconds,
    NO_HINT_RETRY_WAIT,
);

const BUCKET_LIMIT_HEADER: &str = "x-ratelimit-limit";
const BUCKET_REMAINING_HEADER: &str = "x-ratelimit-remaining";
const BUCKET_RESET_AFTER_HEADER: &str = "x-ratelimit-reset-after";
const PAYLOAD_JSON_PART: &str = "payload_json";
const FIRST_FILE_PART: &str = "files[0]";
const UPLOAD_CONTENT_TYPE: &str = "application/octet-stream";

#[derive(Clone, Copy)]
pub(crate) struct SendRecord {
    pub(crate) embed_count: u8,
}

const PLAIN_SEND: SendRecord = SendRecord { embed_count: 0 };

struct Delivered {
    response: GatewayResponse,
    latency_ms: u64,
}

pub struct DiscordClient {
    id: IntegrationId,
    publisher: Arc<dyn EventPublisher>,
    creds: Arc<dyn CredentialsRepo>,
    gateway: Gateway,
    health_tx: HealthTx,
    health_state: Arc<Mutex<DiscordHealthSnapshot>>,
    content_state: Arc<Mutex<DiscordContentSnapshot>>,
    rate_limiter: SharedDiscordRateLimiter,
    webhook_registry_serial: tokio::sync::Mutex<()>,
    status_signal: CollectionRevisionSignal,
}

impl DiscordClient {
    pub(crate) fn integration_id(&self) -> &IntegrationId {
        &self.id
    }

    pub(crate) fn creds(&self) -> &Arc<dyn CredentialsRepo> {
        &self.creds
    }

    pub(crate) fn health_tx(&self) -> &HealthTx {
        &self.health_tx
    }

    pub(crate) fn health_state(&self) -> &Arc<Mutex<DiscordHealthSnapshot>> {
        &self.health_state
    }

    pub(crate) fn content_state(&self) -> &Arc<Mutex<DiscordContentSnapshot>> {
        &self.content_state
    }

    pub(crate) fn webhook_registry_serial(&self) -> &tokio::sync::Mutex<()> {
        &self.webhook_registry_serial
    }

    pub(crate) fn status_signal(&self) -> &CollectionRevisionSignal {
        &self.status_signal
    }

    pub fn new(
        api_base: &str,
        publisher: Arc<dyn EventPublisher>,
        creds: Arc<dyn CredentialsRepo>,
    ) -> Result<Arc<Self>, DiscordError> {
        Self::with_timings(api_base, publisher, creds, GatewayTimings::DEFAULT)
    }

    pub fn with_timings(
        api_base: &str,
        publisher: Arc<dyn EventPublisher>,
        creds: Arc<dyn CredentialsRepo>,
        timings: GatewayTimings,
    ) -> Result<Arc<Self>, DiscordError> {
        let rate_limiter: SharedDiscordRateLimiter =
            Arc::new(Mutex::new(DiscordRateLimiter::new()));
        let gateway = Gateway::new(
            api_base,
            Arc::new(GlobalThrottleGate::new(Arc::clone(&rate_limiter))),
            DISCORD_GATEWAY_PROFILE.with_timings(timings),
            None,
        )
        .map_err(|e| DiscordError::Connect(e.to_string()))?;
        let (health_tx, health_state) = make_health_state();
        let content_state = make_content_state();

        Ok(Arc::new(Self {
            id: crate::DISCORD_INTEGRATION.id,
            publisher,
            creds,
            gateway,
            health_tx,
            health_state,
            content_state: Arc::clone(&content_state),
            rate_limiter,
            webhook_registry_serial: tokio::sync::Mutex::new(()),
            status_signal: CollectionRevisionSignal::new(),
        }))
    }

    pub async fn sync_webhooks(&self) {
        if let Err(error) = self.list_webhooks().await {
            tracing::warn!(%error, "discord webhooks could not be listed at startup");
        }
    }

    pub(crate) fn has_webhooks(&self) -> bool {
        !self.content_state.lock_recover().registry.is_empty()
    }

    pub(crate) async fn load_webhook(&self, name: &str) -> Result<WebhookCredential, DiscordError> {
        let id = crate::webhooks::credential_id(name);
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
            .ok_or_else(|| DiscordError::Credential(format!("missing url in {name}")))?;
        Ok(WebhookCredential {
            name: name.to_owned(),
            url: Secret::new(url),
        })
    }

    async fn load_route(&self, name: &str) -> Result<WebhookRoute, DiscordError> {
        let cred = self.load_webhook(name).await?;
        WebhookRoute::parse(cred.url.expose())
    }

    pub(crate) async fn prepare<T>(
        &self,
        webhook_name: &str,
        stage: impl std::future::Future<Output = Result<T, DiscordError>>,
    ) -> Result<T, DiscordError> {
        match stage.await {
            Ok(prepared) => Ok(prepared),
            Err(err) => {
                self.record_early_failure(webhook_name, &err);
                Err(err)
            }
        }
    }

    fn record_early_failure(&self, webhook_name: &str, err: &DiscordError) {
        let deltas = {
            let mut snap = self.health_state.lock_recover();
            record_unsent_failure(&mut snap)
        };
        for delta in deltas {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "having no subscribers is a normal state"
            )]
            let _ = self.health_tx.send(delta);
        }
        {
            let mut snap = self.content_state.lock_recover();
            record_send(&mut snap, webhook_name, None, false, false);
        }
        publish_failed(self.publisher.as_ref(), webhook_name, err);
    }

    pub async fn post_text(
        &self,
        webhook_name: &str,
        content: &str,
        mentions: MentionPolicy,
    ) -> Result<String, DiscordError> {
        let (route, body) = self
            .prepare(webhook_name, async {
                check_content(content)?;
                let route = self.load_route(webhook_name).await?;
                let body = serde_json::json!({
                    "content": content,
                    "allowed_mentions": mentions.to_wire(),
                });
                Ok((route, body))
            })
            .await?;
        self.execute_post(&route, &body, webhook_name, PLAIN_SEND)
            .await
    }

    pub async fn post_embed(
        &self,
        webhook_name: &str,
        embed: DiscordEmbed,
    ) -> Result<String, DiscordError> {
        let (route, body) = self
            .prepare(webhook_name, async {
                embed.validate()?;
                let route = self.load_route(webhook_name).await?;
                let body = serde_json::json!({ "embeds": [embed_to_wire(&embed)] });
                Ok((route, body))
            })
            .await?;
        let record = SendRecord { embed_count: 1 };
        self.execute_post(&route, &body, webhook_name, record).await
    }

    pub async fn edit_message(
        &self,
        webhook_name: &str,
        message_id: &str,
        content: Option<&str>,
        embed: Option<DiscordEmbed>,
        mentions: MentionPolicy,
    ) -> Result<(), DiscordError> {
        let request = self
            .prepare(webhook_name, async {
                if content.is_none() && embed.is_none() {
                    return Err(DiscordError::Validation(ValidationFault::NothingToEdit));
                }
                if let Some(c) = content {
                    check_content(c)?;
                }
                if let Some(e) = &embed {
                    e.validate()?;
                }
                let route = self.load_route(webhook_name).await?;

                let mut map = serde_json::Map::new();
                if let Some(c) = content {
                    map.insert(
                        "content".to_owned(),
                        serde_json::Value::String(c.to_owned()),
                    );
                    map.insert("allowed_mentions".to_owned(), mentions.to_wire());
                }
                if let Some(e) = &embed {
                    map.insert("embeds".to_owned(), serde_json::json!([embed_to_wire(e)]));
                }
                Ok(route
                    .message(Method::Patch, message_id)?
                    .body(json_body(&serde_json::Value::Object(map))?))
            })
            .await?;
        self.execute_without_reply(request, webhook_name).await
    }

    pub async fn send_file(
        &self,
        webhook_name: &str,
        content: Option<&str>,
        file_name: &str,
        file_bytes: &[u8],
        mentions: MentionPolicy,
    ) -> Result<String, DiscordError> {
        let route = self
            .prepare(webhook_name, async {
                if let Some(c) = content {
                    check_content(c)?;
                }
                self.load_route(webhook_name).await
            })
            .await?;

        let mut payload = serde_json::Map::new();
        payload.insert("allowed_mentions".to_owned(), mentions.to_wire());
        if let Some(c) = content {
            payload.insert(
                "content".to_owned(),
                serde_json::Value::String(c.to_owned()),
            );
        }
        let payload_json = serde_json::Value::Object(payload).to_string();
        let upload = MultipartBody::new()
            .text(PAYLOAD_JSON_PART, payload_json)
            .file(FIRST_FILE_PART, file_name, UPLOAD_CONTENT_TYPE, file_bytes);
        let request = route.execute().class(CallClass::Media).body(upload);
        let delivered = self.send(request, webhook_name, PLAIN_SEND).await?;
        Ok(self.finish_post(webhook_name, &delivered, PLAIN_SEND))
    }

    pub async fn delete_message(
        &self,
        webhook_name: &str,
        message_id: &str,
    ) -> Result<(), DiscordError> {
        let request = self
            .prepare(webhook_name, async {
                let route = self.load_route(webhook_name).await?;
                route.message(Method::Delete, message_id)
            })
            .await?;
        self.execute_without_reply(request, webhook_name).await
    }

    pub(crate) async fn execute_post(
        &self,
        route: &WebhookRoute,
        body: &serde_json::Value,
        webhook_name: &str,
        record: SendRecord,
    ) -> Result<String, DiscordError> {
        let request = self
            .prepare(webhook_name, async {
                Ok(route.execute().body(json_body(body)?))
            })
            .await?;
        let delivered = self.send(request, webhook_name, record).await?;
        Ok(self.finish_post(webhook_name, &delivered, record))
    }

    fn finish_post(&self, webhook_name: &str, delivered: &Delivered, record: SendRecord) -> String {
        let message_id = delivered
            .response
            .json::<serde_json::Value>()
            .ok()
            .and_then(|body| body["id"].as_str().map(str::to_owned))
            .unwrap_or_else(|| {
                tracing::warn!(
                    webhook = webhook_name,
                    "discord accepted the post without a readable message id"
                );
                String::new()
            });
        self.apply_send_result(
            webhook_name,
            delivered.latency_ms,
            true,
            Some(message_id.clone()),
            record,
        );
        publish_posted(
            self.publisher.as_ref(),
            webhook_name,
            &message_id,
            record.embed_count,
        );
        message_id
    }

    async fn execute_without_reply(
        &self,
        request: GatewayRequest,
        webhook_name: &str,
    ) -> Result<(), DiscordError> {
        let delivered = self.send(request, webhook_name, PLAIN_SEND).await?;
        self.apply_send_result(webhook_name, delivered.latency_ms, true, None, PLAIN_SEND);
        Ok(())
    }

    async fn send(
        &self,
        request: GatewayRequest,
        webhook_name: &str,
        record: SendRecord,
    ) -> Result<Delivered, DiscordError> {
        let start = Instant::now();
        if let Err(err) = self.check_pre_send(webhook_name) {
            return Err(self.record_failure(webhook_name, start, err, record));
        }
        let (retry_after, scope) = match self.gateway.send(request.clone()).await {
            Ok(response) => return Ok(self.delivered(webhook_name, response, start)),
            Err(GatewayFailure::RateLimited {
                retry_after, scope, ..
            }) => (retry_after, scope),
            Err(failure) => return Err(self.rejected(webhook_name, &failure, start, record)),
        };
        self.observe_throttle(webhook_name, retry_after, scope);
        publish_rate_limited(self.publisher.as_ref(), webhook_name, retry_after);
        if retry_after > self.gateway.profile().timings.max_throttle_wait {
            return Err(self.rate_limited(webhook_name, retry_after, start, record));
        }

        tokio::time::sleep(retry_after).await;

        let retry_start = Instant::now();
        match self.gateway.send(request).await {
            Ok(response) => Ok(self.delivered(webhook_name, response, retry_start)),
            Err(GatewayFailure::RateLimited {
                retry_after, scope, ..
            }) => {
                self.observe_throttle(webhook_name, retry_after, scope);
                Err(self.rate_limited(webhook_name, retry_after, retry_start, record))
            }
            Err(failure) => Err(self.rejected(webhook_name, &failure, retry_start, record)),
        }
    }

    fn delivered(
        &self,
        webhook_name: &str,
        response: GatewayResponse,
        start: Instant,
    ) -> Delivered {
        if let Some((limit, remaining, reset_after)) = bucket_headers(&response) {
            let mut rl = self.rate_limiter.lock_recover();
            rl.record_response(Instant::now(), webhook_name, limit, remaining, reset_after);
        }
        Delivered {
            response,
            latency_ms: elapsed_ms(start),
        }
    }

    fn rejected(
        &self,
        webhook_name: &str,
        failure: &GatewayFailure,
        start: Instant,
        record: SendRecord,
    ) -> DiscordError {
        if matches!(failure, GatewayFailure::BudgetExhausted { .. }) {
            let global_wait = self
                .rate_limiter
                .lock_recover()
                .global_wait_duration(Instant::now())
                .unwrap_or_default();
            let err = DiscordError::RateLimited {
                retry_after: global_wait,
            };
            return self.record_failure(webhook_name, start, err, record);
        }
        let Some(status) = failure.status() else {
            let err = DiscordError::Connect(failure.to_string());
            return self.record_failure(webhook_name, start, err, record);
        };
        let err = DiscordError::BadResponse {
            status,
            body: failure
                .excerpt()
                .map(|excerpt| excerpt.as_str().to_owned())
                .unwrap_or_default(),
        };
        self.record_failure(webhook_name, start, err, record)
    }

    fn rate_limited(
        &self,
        webhook_name: &str,
        retry_after: Duration,
        start: Instant,
        record: SendRecord,
    ) -> DiscordError {
        let err = DiscordError::RateLimited { retry_after };
        self.record_failure(webhook_name, start, err, record)
    }

    fn record_failure(
        &self,
        webhook_name: &str,
        start: Instant,
        err: DiscordError,
        record: SendRecord,
    ) -> DiscordError {
        self.apply_send_result(webhook_name, elapsed_ms(start), false, None, record);
        publish_failed(self.publisher.as_ref(), webhook_name, &err);
        err
    }

    fn check_pre_send(&self, webhook_name: &str) -> Result<(), DiscordError> {
        let mut rl = self.rate_limiter.lock_recover();
        let now = Instant::now();
        if let Some(wait) = rl.global_wait_duration(now) {
            publish_rate_limited(self.publisher.as_ref(), webhook_name, wait);
            return Err(DiscordError::RateLimited { retry_after: wait });
        }
        if let RateLimitOutcome::Throttled { wait_for } = rl.acquire(now, webhook_name) {
            publish_rate_limited(self.publisher.as_ref(), webhook_name, wait_for);
            return Err(DiscordError::RateLimited {
                retry_after: wait_for,
            });
        }
        Ok(())
    }

    fn observe_throttle(&self, webhook_name: &str, wait: Duration, scope: ThrottleScope) {
        let mut rl = self.rate_limiter.lock_recover();
        match scope {
            ThrottleScope::Global => rl.observe_global_throttle(Instant::now(), wait),
            ThrottleScope::Bucket => rl.observe_remote_throttle(Instant::now(), webhook_name, wait),
        }
    }

    fn apply_send_result(
        &self,
        webhook_name: &str,
        latency_ms: u64,
        ok: bool,
        message_id: Option<String>,
        record: SendRecord,
    ) {
        let (rl_remaining, rl_total) = {
            let rl = self.rate_limiter.lock_recover();
            rl.budget(webhook_name)
        };
        let reset_hint = {
            let rl = self.rate_limiter.lock_recover();
            rl.reset_hint_secs(Instant::now(), webhook_name)
                .and_then(|secs| Duration::try_from_secs_f64(secs.round()).ok())
        };

        let deltas = {
            let mut snap = self.health_state.lock_recover();
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
            #[expect(
                clippy::let_underscore_must_use,
                reason = "having no subscribers is a normal state"
            )]
            let _ = self.health_tx.send(delta);
        }

        {
            let has_embed = record.embed_count > 0;
            let mut snap = self.content_state.lock_recover();
            record_send(&mut snap, webhook_name, message_id, has_embed, ok);
        }
    }
}

#[async_trait::async_trait]
impl DiscordSink for DiscordClient {
    async fn post_text(
        &self,
        webhook_name: &str,
        content: &str,
        mentions: MentionPolicy,
    ) -> Result<String, DiscordError> {
        self.post_text(webhook_name, content, mentions).await
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
        mentions: MentionPolicy,
    ) -> Result<(), DiscordError> {
        self.edit_message(webhook_name, message_id, content, embed, mentions)
            .await
    }

    async fn send_file(
        &self,
        webhook_name: &str,
        content: Option<&str>,
        file_name: &str,
        file_bytes: &[u8],
        mentions: MentionPolicy,
    ) -> Result<String, DiscordError> {
        self.send_file(webhook_name, content, file_name, file_bytes, mentions)
            .await
    }

    async fn delete_message(
        &self,
        webhook_name: &str,
        message_id: &str,
    ) -> Result<(), DiscordError> {
        self.delete_message(webhook_name, message_id).await
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

fn json_body(body: &serde_json::Value) -> Result<RequestBody, DiscordError> {
    RequestBody::json(body)
        .map_err(|e| DiscordError::Validation(ValidationFault::BodyEncoding(e.to_string())))
}

fn bucket_headers(response: &impl HeaderLookup) -> Option<(u32, u32, f64)> {
    let limit = header_number::<u32>(response, BUCKET_LIMIT_HEADER).filter(|limit| *limit > 0)?;
    let remaining = header_number(response, BUCKET_REMAINING_HEADER).unwrap_or(0);
    let reset_after = header_number(response, BUCKET_RESET_AFTER_HEADER).unwrap_or(0.0);
    Some((limit, remaining, reset_after))
}

fn header_number<T: std::str::FromStr>(response: &impl HeaderLookup, name: &str) -> Option<T> {
    response.header_value(name)?.trim().parse().ok()
}

fn check_content(content: &str) -> Result<(), DiscordError> {
    let length = content.chars().count();
    if length > MAX_CONTENT_CHARS {
        return Err(DiscordError::Validation(ValidationFault::ContentTooLong {
            length,
            max: MAX_CONTENT_CHARS,
        }));
    }
    Ok(())
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
pub(crate) mod tests {
    use forge_storage::test_support::MemoryCredentials;

    use std::sync::{Arc, Mutex};

    use forge_storage::CredentialsRepo;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    use crate::embed::{DiscordEmbed, DiscordEmbedField};
    use forge_events::test_support::CapturingPublisher;

    pub(crate) const HOOK_ID: &str = "1300000000000000001";
    pub(crate) const HOOK_TOKEN: &str = "hook-token-Qx7vT2";
    const MESSAGE_ID: &str = "1400000000000000001";
    const UNROUTABLE_API_BASE: &str = "https://192.0.2.1/api";

    fn refused_api_base() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        format!("http://{address}/api")
    }
    const OVER_CAP_HINT_SECS: f64 = 30.0;
    const SHORT_HINT_SECS: f64 = 0.05;

    pub(crate) fn hook_url(host: &str, query: &str) -> String {
        format!("https://{host}/api/webhooks/{HOOK_ID}/{HOOK_TOKEN}{query}")
    }

    fn hook_path() -> String {
        format!("/webhooks/{HOOK_ID}/{HOOK_TOKEN}")
    }

    fn message_path() -> String {
        format!("{}/messages/{MESSAGE_ID}", hook_path())
    }

    impl DiscordClient {
        pub(crate) fn new_for_test() -> Arc<Self> {
            Self::new_for_test_with_creds(Arc::new(MemoryCredentials::default()))
        }

        pub(crate) fn new_for_test_with_creds(creds: Arc<dyn CredentialsRepo>) -> Arc<Self> {
            let rate_limiter: SharedDiscordRateLimiter =
                Arc::new(Mutex::new(DiscordRateLimiter::new()));
            let gateway = Gateway::new(
                UNROUTABLE_API_BASE,
                Arc::new(GlobalThrottleGate::new(Arc::clone(&rate_limiter))),
                DISCORD_GATEWAY_PROFILE,
                None,
            )
            .unwrap();
            let (health_tx, health_state) = make_health_state();
            Arc::new(Self {
                id: crate::DISCORD_INTEGRATION.id,
                publisher: CapturingPublisher::shared(),
                creds,
                gateway,
                health_tx,
                health_state,
                content_state: make_content_state(),
                rate_limiter,
                webhook_registry_serial: tokio::sync::Mutex::new(()),
                status_signal: CollectionRevisionSignal::new(),
            })
        }
    }

    fn store_hook(creds: &MemoryCredentials, name: &str, url: &str) {
        creds.put(
            &format!("{}{name}", crate::credentials::DISCORD_CRED_PREFIX),
            &serde_json::json!({ "url": url }).to_string(),
        );
    }

    fn with_bucket_headers(template: ResponseTemplate, limit: &str) -> ResponseTemplate {
        template
            .insert_header("x-ratelimit-limit", limit)
            .insert_header("x-ratelimit-remaining", "4")
            .insert_header("x-ratelimit-reset-after", "1.0")
    }

    fn make_standard_response(message_id: &str) -> ResponseTemplate {
        with_bucket_headers(
            ResponseTemplate::new(reqwest::StatusCode::OK)
                .set_body_json(serde_json::json!({ "id": message_id })),
            "5",
        )
    }

    fn make_429_response(retry_after_secs: f64) -> ResponseTemplate {
        ResponseTemplate::new(reqwest::StatusCode::TOO_MANY_REQUESTS)
            .set_body_json(serde_json::json!({ "retry_after": retry_after_secs }))
            .insert_header("retry-after", retry_after_secs.to_string().as_str())
    }

    fn client_at(
        api_base: &str,
        publisher: &Arc<CapturingPublisher>,
        creds: &Arc<MemoryCredentials>,
    ) -> Arc<DiscordClient> {
        DiscordClient::new(api_base, publisher.clone(), creds.clone()).unwrap()
    }

    async fn make_client(
        server: &MockServer,
        pub_ref: Arc<CapturingPublisher>,
        creds_ref: Arc<MemoryCredentials>,
    ) -> Arc<DiscordClient> {
        store_hook(&creds_ref, "alerts", &hook_url("discord.com", ""));
        client_at(&server.uri(), &pub_ref, &creds_ref)
    }

    async fn received_count(server: &MockServer) -> usize {
        server.received_requests().await.unwrap().len()
    }

    #[tokio::test]
    async fn post_text_happy_path_returns_message_id() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let creds = Arc::new(MemoryCredentials::default());
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .and(query_param("wait", "true"))
            .respond_with(make_standard_response("msg001"))
            .mount(&server)
            .await;

        let result = client
            .post_text("alerts", "hello world", MentionPolicy::default())
            .await;
        assert_eq!(result.unwrap(), "msg001");
    }

    #[tokio::test]
    async fn post_text_emits_posted_event() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let creds = Arc::new(MemoryCredentials::default());
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .and(query_param("wait", "true"))
            .respond_with(make_standard_response("msg002"))
            .mount(&server)
            .await;

        client
            .post_text("alerts", "hello", MentionPolicy::default())
            .await
            .unwrap();

        let ev = publisher.find_kind("discord.webhook.posted").unwrap();
        assert_eq!(ev.payload["webhook_name"], "alerts");
        assert_eq!(ev.payload["message_id"], "msg002");
        assert_eq!(ev.payload["embed_count"], 0);
    }

    #[test]
    fn a_full_embed_reaches_the_wire_with_the_field_shapes_discord_reads() {
        let embed = DiscordEmbed {
            title: Some("Title".to_owned()),
            description: Some("Body".to_owned()),
            color: Some(0x00_11_22),
            fields: vec![DiscordEmbedField {
                name: "Name".to_owned(),
                value: "Value".to_owned(),
                inline: true,
            }],
            thumbnail_url: Some("https://t.example/thumb.png".to_owned()),
            image_url: Some("https://t.example/image.png".to_owned()),
            footer_text: Some("Footer".to_owned()),
            author_name: Some("Author".to_owned()),
            timestamp: Some(time::OffsetDateTime::from_unix_timestamp(1793449805).unwrap()),
        };

        assert_eq!(
            embed_to_wire(&embed),
            serde_json::json!({
                "title": "Title",
                "description": "Body",
                "color": 0x00_11_22,
                "fields": [{ "name": "Name", "value": "Value", "inline": true }],
                "thumbnail": { "url": "https://t.example/thumb.png" },
                "image": { "url": "https://t.example/image.png" },
                "footer": { "text": "Footer" },
                "author": { "name": "Author" },
                "timestamp": "2026-10-31T12:30:05Z",
            })
        );
    }

    #[test]
    fn an_empty_embed_puts_no_keys_on_the_wire() {
        assert_eq!(
            embed_to_wire(&DiscordEmbed::default()),
            serde_json::json!({})
        );
    }

    #[tokio::test]
    async fn post_embed_returns_message_id_with_embed_count_one() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let creds = Arc::new(MemoryCredentials::default());
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .and(query_param("wait", "true"))
            .respond_with(make_standard_response("msg010"))
            .mount(&server)
            .await;

        let embed = DiscordEmbed {
            title: Some("Test".to_owned()),
            description: Some("Hello".to_owned()),
            ..Default::default()
        };
        assert_eq!(client.post_embed("alerts", embed).await.unwrap(), "msg010");

        let ev = publisher.find_kind("discord.webhook.posted").unwrap();
        assert_eq!(ev.payload["embed_count"], 1);
    }

    #[tokio::test]
    async fn rate_limit_retry_succeeds_after_429() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let creds = Arc::new(MemoryCredentials::default());
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .respond_with(make_429_response(SHORT_HINT_SECS))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .respond_with(make_standard_response("msg_retry"))
            .mount(&server)
            .await;

        let result = client
            .post_text("alerts", "retry test", MentionPolicy::default())
            .await;
        assert_eq!(result.unwrap(), "msg_retry");
        assert_eq!(received_count(&server).await, 2);
        assert!(publisher.has_kind("discord.webhook.rate_limited"));
        assert!(publisher.has_kind("discord.webhook.posted"));
    }

    #[tokio::test]
    async fn double_429_returns_rate_limited_error() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let creds = Arc::new(MemoryCredentials::default());
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .respond_with(make_429_response(SHORT_HINT_SECS))
            .mount(&server)
            .await;

        let result = client
            .post_text("alerts", "double retry", MentionPolicy::default())
            .await;
        assert!(matches!(result, Err(DiscordError::RateLimited { .. })));
        assert_eq!(received_count(&server).await, 2);
        assert!(publisher.has_kind("discord.webhook.failed"));
    }

    #[tokio::test]
    async fn a_second_consecutive_429_holds_the_webhook_until_its_hint_runs_out() {
        let server = MockServer::start().await;
        let client = make_client(
            &server,
            CapturingPublisher::shared(),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        Mock::given(method("POST"))
            .respond_with(make_429_response(SHORT_HINT_SECS))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(make_429_response(OVER_CAP_HINT_SECS))
            .mount(&server)
            .await;

        client
            .post_text("alerts", "first", MentionPolicy::default())
            .await
            .unwrap_err();
        let held = client
            .post_text("alerts", "second", MentionPolicy::default())
            .await;

        assert!(matches!(held, Err(DiscordError::RateLimited { .. })));
        assert_eq!(received_count(&server).await, 2);
    }

    #[tokio::test]
    async fn a_hint_over_the_wait_cap_fails_at_once_without_a_retry() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let client = make_client(
            &server,
            Arc::clone(&publisher),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        Mock::given(method("POST"))
            .respond_with(make_429_response(OVER_CAP_HINT_SECS))
            .mount(&server)
            .await;

        let result = client
            .post_text("alerts", "hi", MentionPolicy::default())
            .await;

        assert!(
            matches!(result, Err(DiscordError::RateLimited { retry_after }) if retry_after.as_secs_f64() == OVER_CAP_HINT_SECS),
            "{result:?}"
        );
        assert_eq!(received_count(&server).await, 1);
        assert!(publisher.has_kind("discord.webhook.rate_limited"));
        assert!(publisher.has_kind("discord.webhook.failed"));
    }

    #[tokio::test]
    async fn a_throttle_scope_decides_whether_other_webhooks_are_held_too() {
        for (scope_header, other_webhook_held) in [
            (("x-ratelimit-global", "true"), true),
            (("x-ratelimit-scope", "global"), true),
            (("x-ratelimit-scope", "user"), false),
            (("x-ratelimit-scope", "shared"), false),
        ] {
            let server = MockServer::start().await;
            let creds = Arc::new(MemoryCredentials::default());
            store_hook(&creds, "clips", &hook_url("discord.com", ""));
            let client =
                make_client(&server, CapturingPublisher::shared(), Arc::clone(&creds)).await;
            Mock::given(method("POST"))
                .respond_with(
                    make_429_response(OVER_CAP_HINT_SECS)
                        .insert_header(scope_header.0, scope_header.1),
                )
                .up_to_n_times(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .respond_with(make_standard_response("msg_other"))
                .mount(&server)
                .await;

            client
                .post_text("alerts", "throttled", MentionPolicy::default())
                .await
                .unwrap_err();
            let same = client
                .post_text("alerts", "again", MentionPolicy::default())
                .await;
            let other = client
                .post_text("clips", "elsewhere", MentionPolicy::default())
                .await;

            assert!(
                matches!(same, Err(DiscordError::RateLimited { .. })),
                "{scope_header:?}: the throttled webhook went out again"
            );
            assert_eq!(
                matches!(other, Err(DiscordError::RateLimited { .. })),
                other_webhook_held,
                "{scope_header:?}"
            );
            let expected_requests = if other_webhook_held { 1 } else { 2 };
            assert_eq!(
                received_count(&server).await,
                expected_requests,
                "{scope_header:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_503_counts_as_a_throttle_only_when_it_carries_retry_after() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let client = make_client(
            &server,
            Arc::clone(&publisher),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::SERVICE_UNAVAILABLE)
                    .insert_header("retry-after", SHORT_HINT_SECS.to_string().as_str()),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(make_standard_response("msg_after_503"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(
                reqwest::StatusCode::SERVICE_UNAVAILABLE,
            ))
            .mount(&server)
            .await;

        let throttled = client
            .post_text("alerts", "hinted", MentionPolicy::default())
            .await;
        let bare = client
            .post_text("alerts", "bare", MentionPolicy::default())
            .await;

        assert_eq!(throttled.unwrap(), "msg_after_503");
        assert!(publisher.has_kind("discord.webhook.rate_limited"));
        assert!(
            matches!(bare, Err(DiscordError::BadResponse { status: 503, .. })),
            "{bare:?}"
        );
    }

    struct ScriptedHttp {
        base: String,
        served: Arc<Mutex<usize>>,
    }

    impl ScriptedHttp {
        fn served(&self) -> usize {
            *self.served.lock().unwrap()
        }
    }

    fn http_reply(status_line: &str, headers: &[(&str, &str)], body: &str) -> String {
        let mut reply = format!("HTTP/1.1 {status_line}\r\n");
        for (name, value) in headers {
            reply.push_str(&format!("{name}: {value}\r\n"));
        }
        reply.push_str(&format!(
            "content-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        ));
        reply
    }

    fn declared_length(head: &[u8]) -> usize {
        String::from_utf8_lossy(head)
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().ok())
                    .flatten()
            })
            .unwrap_or_default()
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) {
        let mut seen = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let read = socket.read(&mut chunk).await.unwrap();
            if read == 0 {
                return;
            }
            seen.extend_from_slice(&chunk[..read]);
            if let Some(end) = seen.windows(4).position(|w| w == b"\r\n\r\n")
                && seen.len() >= end + 4 + declared_length(&seen[..end])
            {
                return;
            }
        }
    }

    async fn scripted_http(replies: Vec<String>) -> ScriptedHttp {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let served = Arc::new(Mutex::new(0));
        let counter = Arc::clone(&served);
        tokio::spawn(async move {
            for reply in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                read_request(&mut socket).await;
                *counter.lock().unwrap() += 1;
                socket.write_all(reply.as_bytes()).await.unwrap();
                socket.shutdown().await.unwrap();
            }
        });
        ScriptedHttp { base, served }
    }

    #[tokio::test]
    async fn a_hint_of_exactly_the_wait_cap_is_waited_out_and_retried() {
        let cap = Duration::from_millis(125);
        for (hint, retried) in [("0.125", true), ("0.126", false)] {
            let stub = scripted_http(vec![
                http_reply("429 Too Many Requests", &[("retry-after", hint)], "{}"),
                http_reply("200 OK", &[], r#"{"id":"msg_cap"}"#),
            ])
            .await;
            let creds = Arc::new(MemoryCredentials::default());
            store_hook(&creds, "alerts", &hook_url("discord.com", ""));
            let client = DiscordClient::with_timings(
                &stub.base,
                CapturingPublisher::shared(),
                creds.clone(),
                GatewayTimings {
                    max_throttle_wait: cap,
                    ..GatewayTimings::DEFAULT
                },
            )
            .unwrap();

            let result = client
                .post_text("alerts", "at the cap", MentionPolicy::default())
                .await;

            assert_eq!(result.is_ok(), retried, "hint {hint}: {result:?}");
            assert_eq!(stub.served(), if retried { 2 } else { 1 }, "hint {hint}");
        }
    }

    #[tokio::test]
    async fn bucket_headers_feed_the_limiter_only_from_a_successful_response() {
        for (reply, expected_budget) in [
            (make_standard_response("msg_bucket"), (4, 5)),
            (
                with_bucket_headers(
                    ResponseTemplate::new(reqwest::StatusCode::OK)
                        .set_body_json(serde_json::json!({ "id": "m" })),
                    "0",
                ),
                (0, 0),
            ),
            (
                with_bucket_headers(
                    ResponseTemplate::new(reqwest::StatusCode::OK)
                        .set_body_json(serde_json::json!({ "id": "m" })),
                    "five",
                ),
                (0, 0),
            ),
            (
                with_bucket_headers(ResponseTemplate::new(reqwest::StatusCode::NOT_FOUND), "5"),
                (0, 0),
            ),
            (
                with_bucket_headers(
                    ResponseTemplate::new(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
                    "5",
                ),
                (0, 0),
            ),
        ] {
            let server = MockServer::start().await;
            let client = make_client(
                &server,
                CapturingPublisher::shared(),
                Arc::new(MemoryCredentials::default()),
            )
            .await;
            Mock::given(method("POST"))
                .respond_with(reply)
                .mount(&server)
                .await;

            let outcome = client
                .post_text("alerts", "bucket test", MentionPolicy::default())
                .await;

            let budget = client.rate_limiter.lock().unwrap().budget("alerts");
            assert_eq!(budget, expected_budget, "{outcome:?}");
        }
    }

    #[tokio::test]
    async fn http_404_returns_bad_response_error_and_emits_failed() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let creds = Arc::new(MemoryCredentials::default());
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::NOT_FOUND).set_body_json(
                    serde_json::json!({
                        "code": 10015,
                        "message": "Unknown Webhook"
                    }),
                ),
            )
            .mount(&server)
            .await;

        let result = client
            .post_text("alerts", "test", MentionPolicy::default())
            .await;
        assert!(matches!(
            result,
            Err(DiscordError::BadResponse { status: 404, .. })
        ));
        assert!(publisher.has_kind("discord.webhook.failed"));
        assert!(!publisher.has_kind("request.fail"));
    }

    #[tokio::test]
    async fn failed_event_payload_carries_reason_status_and_status_only_detail() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let creds = Arc::new(MemoryCredentials::default());
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::NOT_FOUND)
                    .set_body_string("Unknown Webhook"),
            )
            .mount(&server)
            .await;

        client
            .post_text("alerts", "test", MentionPolicy::default())
            .await
            .unwrap_err();

        let ev = publisher.find_kind("discord.webhook.failed").unwrap();
        assert_eq!(ev.payload["reason"], "http_status");
        assert_eq!(ev.payload["status_code"], 404);
        assert_eq!(ev.payload["detail"], "HTTP 404");
    }

    #[tokio::test]
    async fn an_error_body_echoing_the_webhook_id_or_token_never_surfaces_either() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let client = make_client(
            &server,
            Arc::clone(&publisher),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        Mock::given(wiremock::matchers::any())
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::NOT_FOUND).set_body_string(format!(
                    r#"{{"message":"Unknown Webhook {HOOK_ID}/{HOOK_TOKEN}","code":10015}}"#
                )),
            )
            .mount(&server)
            .await;

        let errors = [
            client
                .post_text("alerts", "hi", MentionPolicy::default())
                .await
                .unwrap_err(),
            client
                .edit_message(
                    "alerts",
                    MESSAGE_ID,
                    Some("hi"),
                    None,
                    MentionPolicy::default(),
                )
                .await
                .unwrap_err(),
            client
                .send_file("alerts", None, "a.png", &[1], MentionPolicy::default())
                .await
                .unwrap_err(),
            client
                .delete_message("alerts", MESSAGE_ID)
                .await
                .unwrap_err(),
        ];

        let mut surfaced: Vec<String> = publisher
            .events()
            .iter()
            .map(|event| event.payload.to_string())
            .collect();
        surfaced.extend(errors.iter().map(ToString::to_string));
        assert_eq!(surfaced.len(), 8, "{surfaced:?}");
        for shown in surfaced {
            assert!(!shown.contains(HOOK_TOKEN), "token surfaced: {shown}");
            assert!(!shown.contains(HOOK_ID), "webhook id surfaced: {shown}");
        }
    }

    #[tokio::test]
    async fn the_discord_api_override_carries_webhook_posts_to_its_base() {
        let server = MockServer::start().await;
        let creds = Arc::new(MemoryCredentials::default());
        store_hook(&creds, "alerts", &hook_url("discord.com", ""));
        let override_base = format!("{}/api", server.uri());
        let endpoints = forge_platform_core::PlatformEndpoints::resolve(
            crate::endpoint_surfaces::ENDPOINT_SURFACES,
            |name| {
                (name == crate::endpoint_surfaces::DISCORD_API.env_var())
                    .then(|| override_base.clone().into())
            },
        )
        .unwrap();
        let client = client_at(
            endpoints.base_url(crate::endpoint_surfaces::DISCORD_API),
            &CapturingPublisher::shared(),
            &creds,
        );
        mount_accepting(&server, "POST").await;

        client
            .post_text("alerts", "hi", MentionPolicy::default())
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests[0].url.path(), format!("/api{}", hook_path()));
    }

    #[tokio::test]
    async fn rate_limited_event_payload_carries_webhook_name_and_retry_after() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let creds = Arc::new(MemoryCredentials::default());
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .respond_with(make_429_response(SHORT_HINT_SECS))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .respond_with(make_standard_response("msg_rl"))
            .mount(&server)
            .await;

        client
            .post_text("alerts", "rl test", MentionPolicy::default())
            .await
            .unwrap();

        let ev = publisher.find_kind("discord.webhook.rate_limited").unwrap();
        assert_eq!(ev.payload["webhook_name"], "alerts");
        let ra = ev.payload["retry_after_secs"].as_f64().unwrap();
        assert!((ra - SHORT_HINT_SECS).abs() < 1e-9);
    }

    #[tokio::test]
    async fn edit_message_patch_succeeds() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let creds = Arc::new(MemoryCredentials::default());
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("PATCH"))
            .and(path(message_path()))
            .respond_with(make_standard_response(MESSAGE_ID))
            .mount(&server)
            .await;

        let result = client
            .edit_message(
                "alerts",
                MESSAGE_ID,
                Some("updated"),
                None,
                MentionPolicy::default(),
            )
            .await;
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(request_bodies(&server).await[0]["content"], "updated");
    }

    #[tokio::test]
    async fn a_stored_endpoint_outside_the_webhook_route_is_refused_before_any_request() {
        for (id, token) in [
            ("12a4", HOOK_TOKEN),
            ("+123", HOOK_TOKEN),
            ("\u{0661}\u{0662}\u{0663}", HOOK_TOKEN),
            ("18446744073709551616", HOOK_TOKEN),
            ("", HOOK_TOKEN),
            (HOOK_ID, ".."),
            (HOOK_ID, "tok..en"),
            (HOOK_ID, "%2e%2e"),
            (HOOK_ID, "tok%2Fen"),
            (HOOK_ID, ""),
        ] {
            let server = MockServer::start().await;
            let creds = Arc::new(MemoryCredentials::default());
            store_hook(
                &creds,
                "alerts",
                &format!("https://discord.com/api/webhooks/{id}/{token}"),
            );
            let client = client_at(&server.uri(), &CapturingPublisher::shared(), &creds);

            let err = client
                .post_text("alerts", "hi", MentionPolicy::default())
                .await
                .unwrap_err();

            assert!(
                matches!(err, DiscordError::Validation(_)),
                "{id:?}/{token:?}: {err:?}"
            );
            assert!(!err.to_string().contains(HOOK_TOKEN), "{err}");
            assert_eq!(received_count(&server).await, 0, "{id:?}/{token:?}");
        }
    }

    #[tokio::test]
    async fn a_message_id_that_is_not_a_snowflake_is_refused_before_any_request() {
        let server = MockServer::start().await;
        let client = make_client(
            &server,
            CapturingPublisher::shared(),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        Mock::given(wiremock::matchers::any())
            .respond_with(make_standard_response(MESSAGE_ID))
            .mount(&server)
            .await;

        for bad in [
            "",
            "abc",
            "12a",
            "+1",
            "18446744073709551616",
            "1/2",
            "..",
            "1%2F2",
            "\u{0661}",
        ] {
            let edited = client
                .edit_message("alerts", bad, Some("x"), None, MentionPolicy::default())
                .await;
            let deleted = client.delete_message("alerts", bad).await;

            assert!(
                matches!(edited, Err(DiscordError::Validation(_))),
                "edit {bad:?}: {edited:?}"
            );
            assert!(
                matches!(deleted, Err(DiscordError::Validation(_))),
                "delete {bad:?}: {deleted:?}"
            );
        }
        assert_eq!(received_count(&server).await, 0);
    }

    type SentRequest = (String, String, Vec<(String, String)>);

    fn sorted_query(request: &wiremock::Request) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = request
            .url
            .query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        pairs.sort();
        pairs
    }

    fn owned_pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[tokio::test]
    async fn every_webhook_host_posts_to_the_api_base_with_its_saved_thread() {
        for host in [
            "discord.com",
            "discordapp.com",
            "canary.discord.com",
            "ptb.discord.com",
        ] {
            let server = MockServer::start().await;
            let creds = Arc::new(MemoryCredentials::default());
            store_hook(
                &creds,
                "alerts",
                &hook_url(host, "?wait=false&thread_id=987"),
            );
            let client = client_at(&server.uri(), &CapturingPublisher::shared(), &creds);
            mount_accepting(&server, "POST").await;

            client
                .post_text("alerts", "hi", MentionPolicy::default())
                .await
                .unwrap();

            let requests = server.received_requests().await.unwrap();
            assert_eq!(requests[0].url.path(), hook_path(), "{host}");
            assert_eq!(
                sorted_query(&requests[0]),
                owned_pairs(&[("thread_id", "987"), ("wait", "true")]),
                "{host}"
            );
        }
    }

    #[tokio::test]
    async fn edit_and_delete_address_the_message_under_the_saved_thread() {
        let server = MockServer::start().await;
        let creds = Arc::new(MemoryCredentials::default());
        store_hook(
            &creds,
            "alerts",
            &hook_url("discordapp.com", "?thread_id=987"),
        );
        let client = client_at(&server.uri(), &CapturingPublisher::shared(), &creds);
        Mock::given(wiremock::matchers::any())
            .respond_with(make_standard_response(MESSAGE_ID))
            .mount(&server)
            .await;

        client
            .edit_message(
                "alerts",
                MESSAGE_ID,
                Some("edited"),
                None,
                MentionPolicy::default(),
            )
            .await
            .unwrap();
        client.delete_message("alerts", MESSAGE_ID).await.unwrap();

        let sent: Vec<SentRequest> = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r.method.to_string(),
                    r.url.path().to_owned(),
                    sorted_query(r),
                )
            })
            .collect();
        let thread = owned_pairs(&[("thread_id", "987")]);
        assert_eq!(
            sent,
            [
                ("PATCH".to_owned(), message_path(), thread.clone()),
                ("DELETE".to_owned(), message_path(), thread),
            ]
        );
    }

    #[tokio::test]
    async fn transport_failures_surface_as_connect_without_the_endpoint() {
        let creds = Arc::new(MemoryCredentials::default());
        store_hook(&creds, "leaky", &hook_url("discord.com", ""));
        let client = client_at(&refused_api_base(), &CapturingPublisher::shared(), &creds);
        let embed = DiscordEmbed {
            title: Some("t".to_owned()),
            ..Default::default()
        };

        let errors = [
            client
                .post_text("leaky", "hi", MentionPolicy::default())
                .await
                .unwrap_err(),
            client.post_embed("leaky", embed).await.unwrap_err(),
            client
                .edit_message(
                    "leaky",
                    MESSAGE_ID,
                    Some("hi"),
                    None,
                    MentionPolicy::default(),
                )
                .await
                .unwrap_err(),
            client
                .send_file("leaky", Some("hi"), "a.png", &[1], MentionPolicy::default())
                .await
                .unwrap_err(),
            client
                .delete_message("leaky", MESSAGE_ID)
                .await
                .unwrap_err(),
        ];

        for err in errors {
            assert!(matches!(err, DiscordError::Connect(_)), "{err:?}");
            let shown = err.to_string();
            assert!(!shown.contains(HOOK_TOKEN), "token leaked: {shown}");
            assert!(!shown.contains(HOOK_ID), "webhook id leaked: {shown}");
        }
    }

    #[tokio::test]
    async fn a_transport_failure_reaches_health_recent_posts_and_the_failed_event() {
        let creds = Arc::new(MemoryCredentials::default());
        store_hook(&creds, "alerts", &hook_url("discord.com", ""));
        let publisher = CapturingPublisher::shared();
        let client = client_at(&refused_api_base(), &publisher, &creds);

        client
            .post_text("alerts", "hi", MentionPolicy::default())
            .await
            .unwrap_err();

        assert_eq!(client.send_health().last_send_ok, Some(false));
        let post = client.recent_posts().remove(0);
        assert_eq!(post.webhook_name, "alerts");
        assert!(!post.ok);
        let failed = publisher.find_kind("discord.webhook.failed").unwrap();
        assert!(!failed.payload.to_string().contains(HOOK_TOKEN));
    }

    #[tokio::test]
    async fn a_global_throttle_refuses_the_send_and_reports_it_everywhere_without_a_request() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let client = make_client(
            &server,
            Arc::clone(&publisher),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        client
            .rate_limiter
            .lock()
            .unwrap()
            .observe_global_throttle(Instant::now(), Duration::from_secs(30));

        let err = client
            .post_text("alerts", "hi", MentionPolicy::default())
            .await
            .unwrap_err();

        assert!(matches!(err, DiscordError::RateLimited { .. }), "{err:?}");
        let throttled = publisher.find_kind("discord.webhook.rate_limited").unwrap();
        assert_eq!(throttled.payload["webhook_name"], "alerts");
        assert!(publisher.has_kind("discord.webhook.failed"));
        assert!(!client.recent_posts().remove(0).ok);
        assert_eq!(received_count(&server).await, 0);
    }

    enum EarlyFailure {
        OverLongText,
        InvalidEmbed,
        EditWithNothing,
        OverLongCaption,
        DeleteOfNonSnowflake,
        StoredUrlOutsideRoute,
        MissingCredential,
        TestToMalformedUrl,
    }

    impl EarlyFailure {
        fn stored_url(&self) -> Option<String> {
            match self {
                Self::MissingCredential => None,
                Self::StoredUrlOutsideRoute => Some(format!(
                    "https://discord.com/api/webhooks/12a4/{HOOK_TOKEN}"
                )),
                _ => Some(hook_url("discord.com", "")),
            }
        }

        async fn attempt(&self, client: &DiscordClient) -> DiscordError {
            let over_limit = "x".repeat(MAX_CONTENT_CHARS + 1);
            let policy = MentionPolicy::default();
            let outcome = match self {
                Self::OverLongText => client.post_text("alerts", &over_limit, policy).await,
                Self::InvalidEmbed => {
                    let embed = DiscordEmbed {
                        title: Some("bad\0null".to_owned()),
                        ..Default::default()
                    };
                    client.post_embed("alerts", embed).await
                }
                Self::EditWithNothing => client
                    .edit_message("alerts", MESSAGE_ID, None, None, policy)
                    .await
                    .map(|()| String::new()),
                Self::OverLongCaption => {
                    client
                        .send_file("alerts", Some(&over_limit), "a.png", &[1], policy)
                        .await
                }
                Self::DeleteOfNonSnowflake => client
                    .delete_message("alerts", "abc")
                    .await
                    .map(|()| String::new()),
                Self::StoredUrlOutsideRoute | Self::MissingCredential => {
                    client.post_text("alerts", "hi", policy).await
                }
                Self::TestToMalformedUrl => {
                    client.post_test("alerts", "not a url at all", "ping").await
                }
            };
            outcome.unwrap_err()
        }

        fn expects(&self, err: &DiscordError) -> bool {
            match self {
                Self::OverLongText | Self::OverLongCaption => matches!(
                    err,
                    DiscordError::Validation(ValidationFault::ContentTooLong { .. })
                ),
                Self::EditWithNothing => matches!(
                    err,
                    DiscordError::Validation(ValidationFault::NothingToEdit)
                ),
                Self::MissingCredential => matches!(err, DiscordError::WebhookNotFound { .. }),
                Self::InvalidEmbed
                | Self::DeleteOfNonSnowflake
                | Self::StoredUrlOutsideRoute
                | Self::TestToMalformedUrl => matches!(err, DiscordError::Validation(_)),
            }
        }
    }

    const EARLY_FAILURES: [EarlyFailure; 8] = [
        EarlyFailure::OverLongText,
        EarlyFailure::InvalidEmbed,
        EarlyFailure::EditWithNothing,
        EarlyFailure::OverLongCaption,
        EarlyFailure::DeleteOfNonSnowflake,
        EarlyFailure::StoredUrlOutsideRoute,
        EarlyFailure::MissingCredential,
        EarlyFailure::TestToMalformedUrl,
    ];

    struct EarlyFailureRun {
        server: MockServer,
        publisher: Arc<CapturingPublisher>,
        client: Arc<DiscordClient>,
        err: DiscordError,
    }

    async fn run_early_failure(case: &EarlyFailure) -> EarlyFailureRun {
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::any())
            .respond_with(make_standard_response(MESSAGE_ID))
            .mount(&server)
            .await;
        let creds = Arc::new(MemoryCredentials::default());
        if let Some(url) = case.stored_url() {
            store_hook(&creds, "alerts", &url);
        }
        let publisher = CapturingPublisher::shared();
        let client = client_at(&server.uri(), &publisher, &creds);
        let err = case.attempt(&client).await;
        EarlyFailureRun {
            server,
            publisher,
            client,
            err,
        }
    }

    #[tokio::test]
    async fn a_send_refused_before_the_request_publishes_exactly_one_failed_event() {
        for (index, case) in EARLY_FAILURES.iter().enumerate() {
            let run = run_early_failure(case).await;

            assert!(case.expects(&run.err), "case {index}: {:?}", run.err);
            assert_eq!(received_count(&run.server).await, 0, "case {index}");
            assert_eq!(
                run.publisher.kinds(),
                ["discord.webhook.failed"],
                "case {index}"
            );
        }
    }

    #[tokio::test]
    async fn a_send_refused_before_the_request_counts_once_as_a_failed_send() {
        for (index, case) in EARLY_FAILURES.iter().enumerate() {
            let run = run_early_failure(case).await;

            let health = run.client.send_health();
            assert_eq!(
                (health.last_send_ok, health.errors_last_hour),
                (Some(false), 1),
                "case {index}"
            );
            let posts = run.client.recent_posts();
            assert_eq!(posts.len(), 1, "case {index}");
            assert!(!posts[0].ok, "case {index}");
            assert_eq!(
                run.client
                    .content_state()
                    .lock_recover()
                    .registry
                    .last_ok("alerts"),
                Some(false),
                "case {index}"
            );
        }
    }

    #[tokio::test]
    async fn message_content_is_limited_in_characters_on_every_send_path() {
        let server = MockServer::start().await;
        let client = make_client(
            &server,
            CapturingPublisher::shared(),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        let at_limit = "я".repeat(MAX_CONTENT_CHARS);
        let over_limit = "я".repeat(MAX_CONTENT_CHARS + 1);
        mount_accepting(&server, "POST").await;
        mount_accepting(&server, "PATCH").await;
        let policy = MentionPolicy::default;

        client
            .post_text("alerts", &at_limit, policy())
            .await
            .expect("post at the limit");
        client
            .edit_message("alerts", MESSAGE_ID, Some(&at_limit), None, policy())
            .await
            .expect("edit at the limit");
        client
            .send_file("alerts", Some(&at_limit), "a.png", &[1], policy())
            .await
            .expect("caption at the limit");
        let accepted = received_count(&server).await;

        let refusals = [
            client
                .post_text("alerts", &over_limit, policy())
                .await
                .unwrap_err(),
            client
                .edit_message("alerts", MESSAGE_ID, Some(&over_limit), None, policy())
                .await
                .unwrap_err(),
            client
                .send_file("alerts", Some(&over_limit), "a.png", &[1], policy())
                .await
                .unwrap_err(),
        ];

        for err in refusals {
            assert!(
                matches!(
                    err,
                    DiscordError::Validation(ValidationFault::ContentTooLong {
                        length,
                        max: MAX_CONTENT_CHARS,
                    }) if length == MAX_CONTENT_CHARS + 1
                ),
                "{err:?}"
            );
        }
        assert_eq!(received_count(&server).await, accepted);
    }

    fn mention_cases() -> [(MentionPolicy, serde_json::Value); 4] {
        let policy = |allow_roles, allow_everyone| MentionPolicy {
            allow_roles,
            allow_everyone,
        };
        [
            (
                policy(true, false),
                serde_json::json!({ "parse": ["users", "roles"] }),
            ),
            (
                policy(true, true),
                serde_json::json!({ "parse": ["users", "roles", "everyone"] }),
            ),
            (
                policy(false, false),
                serde_json::json!({ "parse": ["users"] }),
            ),
            (
                policy(false, true),
                serde_json::json!({ "parse": ["users", "everyone"] }),
            ),
        ]
    }

    async fn request_bodies(server: &MockServer) -> Vec<serde_json::Value> {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| serde_json::from_slice(&request.body).unwrap())
            .collect()
    }

    async fn mount_accepting(server: &MockServer, verb: &str) {
        Mock::given(method(verb))
            .respond_with(make_standard_response("msg_mentions"))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn post_text_sends_the_allowed_mentions_of_each_toggle_combination() {
        let server = MockServer::start().await;
        let client = make_client(
            &server,
            CapturingPublisher::shared(),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        mount_accepting(&server, "POST").await;

        for (policy, _) in mention_cases() {
            client
                .post_text("alerts", "<@&42> @everyone live", policy)
                .await
                .unwrap();
        }

        let sent: Vec<serde_json::Value> = request_bodies(&server)
            .await
            .into_iter()
            .map(|body| body["allowed_mentions"].clone())
            .collect();
        let expected: Vec<serde_json::Value> =
            mention_cases().into_iter().map(|(_, wire)| wire).collect();
        assert_eq!(sent, expected);
    }

    #[tokio::test]
    async fn post_text_without_a_policy_pings_users_and_roles_but_never_everyone() {
        let server = MockServer::start().await;
        let client = make_client(
            &server,
            CapturingPublisher::shared(),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        mount_accepting(&server, "POST").await;

        client
            .post_text("alerts", "@everyone test", MentionPolicy::default())
            .await
            .unwrap();

        let bodies = request_bodies(&server).await;
        assert_eq!(
            bodies[0]["allowed_mentions"],
            serde_json::json!({ "parse": ["users", "roles"] })
        );
    }

    #[tokio::test]
    async fn post_test_pings_users_and_roles_but_never_everyone() {
        let server = MockServer::start().await;
        let client = make_client(
            &server,
            CapturingPublisher::shared(),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        mount_accepting(&server, "POST").await;

        client
            .post_test("alerts", &hook_url("discord.com", ""), "@everyone test")
            .await
            .unwrap();

        let bodies = request_bodies(&server).await;
        assert_eq!(
            bodies[0]["allowed_mentions"],
            serde_json::json!({ "parse": ["users", "roles"] })
        );
    }

    #[tokio::test]
    async fn edit_message_sends_the_allowed_mentions_of_each_toggle_combination() {
        let server = MockServer::start().await;
        let client = make_client(
            &server,
            CapturingPublisher::shared(),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        mount_accepting(&server, "PATCH").await;

        for (policy, _) in mention_cases() {
            client
                .edit_message("alerts", MESSAGE_ID, Some("<@&42> live"), None, policy)
                .await
                .unwrap();
        }

        let sent: Vec<serde_json::Value> = request_bodies(&server)
            .await
            .into_iter()
            .map(|body| body["allowed_mentions"].clone())
            .collect();
        let expected: Vec<serde_json::Value> =
            mention_cases().into_iter().map(|(_, wire)| wire).collect();
        assert_eq!(sent, expected);
    }

    #[tokio::test]
    async fn edit_message_with_only_an_embed_sends_no_allowed_mentions() {
        let server = MockServer::start().await;
        let client = make_client(
            &server,
            CapturingPublisher::shared(),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        mount_accepting(&server, "PATCH").await;
        let embed = DiscordEmbed {
            title: Some("live".to_owned()),
            ..Default::default()
        };

        client
            .edit_message(
                "alerts",
                MESSAGE_ID,
                None,
                Some(embed),
                MentionPolicy {
                    allow_roles: true,
                    allow_everyone: true,
                },
            )
            .await
            .unwrap();

        let body = &request_bodies(&server).await[0];
        assert!(body.get("allowed_mentions").is_none(), "{body}");
        assert!(body.get("content").is_none(), "{body}");
    }

    #[tokio::test]
    async fn post_embed_sends_no_allowed_mentions() {
        let server = MockServer::start().await;
        let client = make_client(
            &server,
            CapturingPublisher::shared(),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        mount_accepting(&server, "POST").await;
        let embed = DiscordEmbed {
            description: Some("<@&42> @everyone".to_owned()),
            ..Default::default()
        };

        client.post_embed("alerts", embed).await.unwrap();

        let body = &request_bodies(&server).await[0];
        assert!(body.get("allowed_mentions").is_none(), "{body}");
    }

    struct FormPart {
        head: String,
        body: Vec<u8>,
    }

    fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
        haystack
            .get(from..)?
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|at| at + from)
    }

    fn form_parts(request: &wiremock::Request) -> Vec<FormPart> {
        let content_type = request.headers["content-type"].to_str().unwrap();
        let boundary = content_type.split("boundary=").nth(1).unwrap();
        let delimiter = format!("\r\n--{boundary}").into_bytes();
        let mut framed = b"\r\n".to_vec();
        framed.extend_from_slice(&request.body);
        let mut parts = Vec::new();
        let mut at = find(&framed, &delimiter, 0).unwrap() + delimiter.len();
        while framed.get(at..at + 2) == Some(b"\r\n") {
            let head_end = find(&framed, b"\r\n\r\n", at).unwrap();
            let next = find(&framed, &delimiter, head_end).unwrap();
            parts.push(FormPart {
                head: String::from_utf8_lossy(&framed[at + 2..head_end]).to_lowercase(),
                body: framed[head_end + 4..next].to_vec(),
            });
            at = next + delimiter.len();
        }
        parts
    }

    fn part<'a>(parts: &'a [FormPart], name: &str) -> &'a FormPart {
        let marker = format!("name=\"{name}\"");
        parts.iter().find(|p| p.head.contains(&marker)).unwrap()
    }

    #[tokio::test]
    async fn send_file_caption_payload_carries_default_allowed_mentions() {
        let server = MockServer::start().await;
        let client = make_client(
            &server,
            CapturingPublisher::shared(),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        mount_accepting(&server, "POST").await;

        client
            .send_file(
                "alerts",
                Some("@everyone clip"),
                "clip.png",
                &[1, 2],
                MentionPolicy::default(),
            )
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        let parts = form_parts(&requests[0]);
        let payload: serde_json::Value =
            serde_json::from_slice(&part(&parts, "payload_json").body).unwrap();
        assert_eq!(
            payload,
            serde_json::json!({
                "content": "@everyone clip",
                "allowed_mentions": { "parse": ["users", "roles"] }
            })
        );
    }

    #[tokio::test]
    async fn send_file_uploads_the_bytes_as_the_first_octet_stream_file_part() {
        let server = MockServer::start().await;
        let client = make_client(
            &server,
            CapturingPublisher::shared(),
            Arc::new(MemoryCredentials::default()),
        )
        .await;
        mount_accepting(&server, "POST").await;
        let bytes = [0, 13, 10, b'-', b'-', 255];

        client
            .send_file("alerts", None, "clip.png", &bytes, MentionPolicy::default())
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        let parts = form_parts(&requests[0]);
        let file = part(&parts, "files[0]");
        assert_eq!(parts.len(), 2);
        assert!(file.head.contains("filename=\"clip.png\""), "{}", file.head);
        assert!(
            file.head.contains("content-type: application/octet-stream"),
            "{}",
            file.head
        );
        assert_eq!(file.body, bytes);
    }

    #[tokio::test]
    async fn embed_with_fields_serialized_correctly() {
        let server = MockServer::start().await;
        let publisher = CapturingPublisher::shared();
        let creds = Arc::new(MemoryCredentials::default());
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .and(query_param("wait", "true"))
            .respond_with(make_standard_response("msg_fields"))
            .mount(&server)
            .await;

        let embed = DiscordEmbed {
            title: Some("Embed with fields".to_owned()),
            fields: vec![DiscordEmbedField {
                name: "key".to_owned(),
                value: "value".to_owned(),
                inline: true,
            }],
            ..Default::default()
        };
        client.post_embed("alerts", embed).await.unwrap();

        let body = &request_bodies(&server).await[0];
        assert_eq!(
            body["embeds"][0]["fields"],
            serde_json::json!([{ "name": "key", "value": "value", "inline": true }])
        );
    }
}
