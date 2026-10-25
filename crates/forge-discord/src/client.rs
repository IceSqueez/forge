use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_events::EventPublisher;
use forge_net::{
    CallClass, Gateway, GatewayFailure, GatewayProfile, GatewayRequest, GatewayResponse,
    HeaderLookup, MAX_THROTTLE_WAIT, Method, MultipartBody, RequestBody, ThrottleHintFamily,
    ThrottleScope,
};
use forge_platform_core::ConnectionState;
use forge_storage::{CredentialId, CredentialsRepo};
use forge_types::IntegrationId;
use tokio::sync::broadcast;

use crate::content::{DiscordContentSnapshot, make_content_state, record_send, record_test_send};
use crate::credentials::{DISCORD_CRED_PREFIX, WebhookCredential};
use crate::embed::DiscordEmbed;
use crate::error::DiscordError;
use crate::events::{publish_failed, publish_posted, publish_rate_limited};
use crate::health::{
    DiscordHealthSnapshot, make_health_state, record_missing_webhook, update_on_send,
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
    pub(crate) register_name: bool,
}

const PLAIN_SEND: SendRecord = SendRecord {
    embed_count: 0,
    register_name: true,
};

struct Delivered {
    response: GatewayResponse,
    latency_ms: u64,
}

pub struct DiscordClient {
    pub(crate) id: IntegrationId,
    pub(crate) publisher: Arc<dyn EventPublisher>,
    pub(crate) creds: Arc<dyn CredentialsRepo>,
    pub(crate) gateway: Gateway,
    pub(crate) health_tx: HealthTx,
    pub(crate) health_state: Arc<Mutex<DiscordHealthSnapshot>>,
    pub(crate) content_state: Arc<Mutex<DiscordContentSnapshot>>,
    pub(crate) rate_limiter: SharedDiscordRateLimiter,
    pub(crate) webhook_registry_serial: tokio::sync::Mutex<()>,
}

impl DiscordClient {
    pub fn new(
        api_base: &str,
        publisher: Arc<dyn EventPublisher>,
        creds: Arc<dyn CredentialsRepo>,
    ) -> Result<Arc<Self>, DiscordError> {
        let rate_limiter: SharedDiscordRateLimiter =
            Arc::new(Mutex::new(DiscordRateLimiter::new()));
        let gateway = Gateway::new(
            api_base,
            Arc::new(GlobalThrottleGate::new(Arc::clone(&rate_limiter))),
            DISCORD_GATEWAY_PROFILE,
            None,
        )
        .map_err(|e| DiscordError::Connect(e.to_string()))?;
        let (health_tx, health_state) = make_health_state();
        let content_state = make_content_state();

        let client = Arc::new(Self {
            id: crate::DISCORD_INTEGRATION.id,
            publisher,
            creds,
            gateway,
            health_tx,
            health_state,
            content_state: Arc::clone(&content_state),
            rate_limiter,
            webhook_registry_serial: tokio::sync::Mutex::new(()),
        });

        let boot_sync = Arc::clone(&client);
        tokio::spawn(async move {
            let _ = boot_sync.list_webhooks().await;
        });

        Ok(client)
    }

    pub(crate) fn connection_state(&self) -> ConnectionState {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        if snap.webhook_names.is_empty() {
            ConnectionState::Disconnected
        } else {
            ConnectionState::Connected
        }
    }

    pub(crate) async fn load_webhook(&self, name: &str) -> Result<WebhookCredential, DiscordError> {
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

    async fn load_route(&self, name: &str) -> Result<WebhookRoute, DiscordError> {
        let cred = match self.load_webhook(name).await {
            Ok(cred) => cred,
            Err(err) => {
                self.record_missing_webhook_failure(name, &err);
                return Err(err);
            }
        };
        WebhookRoute::parse(&cred.url)
    }

    fn record_missing_webhook_failure(&self, webhook_name: &str, err: &DiscordError) {
        let deltas = {
            let mut snap = self.health_state.lock().unwrap_or_else(|p| p.into_inner());
            record_missing_webhook(&mut snap)
        };
        for delta in deltas {
            let _ = self.health_tx.send(delta);
        }
        {
            let mut snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
            record_test_send(&mut snap, webhook_name, None, false, false);
        }
        publish_failed(self.publisher.as_ref(), webhook_name, err);
    }

    pub async fn post_text(
        &self,
        webhook_name: &str,
        content: &str,
    ) -> Result<String, DiscordError> {
        self.post_text_with_mentions(webhook_name, content, MentionPolicy::default())
            .await
    }

    pub async fn post_text_with_mentions(
        &self,
        webhook_name: &str,
        content: &str,
        mentions: MentionPolicy,
    ) -> Result<String, DiscordError> {
        let route = self.load_route(webhook_name).await?;
        let body = serde_json::json!({
            "content": content,
            "allowed_mentions": mentions.to_wire(),
        });
        self.execute_post(&route, &body, webhook_name, PLAIN_SEND)
            .await
    }

    pub async fn post_embed(
        &self,
        webhook_name: &str,
        embed: DiscordEmbed,
    ) -> Result<String, DiscordError> {
        embed.validate()?;
        let route = self.load_route(webhook_name).await?;
        let body = serde_json::json!({ "embeds": [embed_to_wire(&embed)] });
        let record = SendRecord {
            embed_count: 1,
            register_name: true,
        };
        self.execute_post(&route, &body, webhook_name, record).await
    }

    pub async fn edit_message(
        &self,
        webhook_name: &str,
        message_id: &str,
        content: Option<&str>,
        embed: Option<DiscordEmbed>,
    ) -> Result<(), DiscordError> {
        self.edit_message_with_mentions(
            webhook_name,
            message_id,
            content,
            embed,
            MentionPolicy::default(),
        )
        .await
    }

    pub async fn edit_message_with_mentions(
        &self,
        webhook_name: &str,
        message_id: &str,
        content: Option<&str>,
        embed: Option<DiscordEmbed>,
        mentions: MentionPolicy,
    ) -> Result<(), DiscordError> {
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
        let request = route
            .message(Method::Patch, message_id)?
            .body(json_body(&serde_json::Value::Object(map))?);
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
        let route = self.load_route(webhook_name).await?;

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
        let route = self.load_route(webhook_name).await?;
        self.execute_without_reply(route.message(Method::Delete, message_id)?, webhook_name)
            .await
    }

    pub(crate) async fn execute_post(
        &self,
        route: &WebhookRoute,
        body: &serde_json::Value,
        webhook_name: &str,
        record: SendRecord,
    ) -> Result<String, DiscordError> {
        let request = route.execute().body(json_body(body)?);
        let delivered = self.send(request, webhook_name, record).await?;
        Ok(self.finish_post(webhook_name, &delivered, record))
    }

    fn finish_post(&self, webhook_name: &str, delivered: &Delivered, record: SendRecord) -> String {
        let message_id = delivered
            .response
            .json::<serde_json::Value>()
            .ok()
            .and_then(|body| body["id"].as_str().map(str::to_owned))
            .unwrap_or_default();
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
        self.check_pre_send(webhook_name)?;
        let start = Instant::now();
        let (retry_after, scope) = match self.gateway.send(request.clone()).await {
            Ok(response) => return Ok(self.delivered(webhook_name, response, start)),
            Err(GatewayFailure::RateLimited {
                retry_after, scope, ..
            }) => (retry_after, scope),
            Err(failure) => return Err(self.rejected(webhook_name, &failure, start, record)),
        };
        self.observe_throttle(webhook_name, retry_after, scope);
        publish_rate_limited(
            self.publisher.as_ref(),
            webhook_name,
            retry_after.as_secs_f64(),
        );
        if retry_after > MAX_THROTTLE_WAIT {
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
            let mut rl = self.rate_limiter.lock().unwrap_or_else(|p| p.into_inner());
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
        if matches!(failure, GatewayFailure::BudgetExhausted) {
            let global_wait = self
                .rate_limiter
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .global_wait_duration(Instant::now())
                .unwrap_or_default();
            return DiscordError::RateLimited {
                retry_after_secs: global_wait.as_secs_f64(),
            };
        }
        let Some(status) = failure.status() else {
            return DiscordError::Connect(failure.to_string());
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
        let err = DiscordError::RateLimited {
            retry_after_secs: retry_after.as_secs_f64(),
        };
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
        let mut rl = self.rate_limiter.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        if let Some(wait) = rl.global_wait_duration(now) {
            let ra = wait.as_secs_f64();
            return Err(DiscordError::RateLimited {
                retry_after_secs: ra,
            });
        }
        if let RateLimitOutcome::Throttled { wait_for } = rl.acquire(now, webhook_name) {
            let ra = wait_for.as_secs_f64();
            publish_rate_limited(self.publisher.as_ref(), webhook_name, ra);
            return Err(DiscordError::RateLimited {
                retry_after_secs: ra,
            });
        }
        Ok(())
    }

    fn observe_throttle(&self, webhook_name: &str, wait: Duration, scope: ThrottleScope) {
        let mut rl = self.rate_limiter.lock().unwrap_or_else(|p| p.into_inner());
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
            let rl = self.rate_limiter.lock().unwrap_or_else(|p| p.into_inner());
            rl.budget(webhook_name)
        };
        let reset_hint = {
            let rl = self.rate_limiter.lock().unwrap_or_else(|p| p.into_inner());
            rl.reset_hint_secs(Instant::now(), webhook_name)
                .map(|s| format!("{s:.0}"))
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
            let has_embed = record.embed_count > 0;
            let mut snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
            if record.register_name {
                record_send(&mut snap, webhook_name, message_id, has_embed, ok);
            } else {
                record_test_send(&mut snap, webhook_name, message_id, has_embed, ok);
            }
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
        self.post_text_with_mentions(webhook_name, content, mentions)
            .await
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
        self.edit_message_with_mentions(webhook_name, message_id, content, embed, mentions)
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
    RequestBody::json(body).map_err(|e| DiscordError::Validation(e.to_string()))
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

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_events::{Event, EventPublisher};
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use time::OffsetDateTime;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::embed::{DiscordEmbed, DiscordEmbedField};

    pub(crate) const HOOK_ID: &str = "1300000000000000001";
    pub(crate) const HOOK_TOKEN: &str = "hook-token-Qx7vT2";
    const MESSAGE_ID: &str = "1400000000000000001";
    const UNROUTABLE_API_BASE: &str = "https://192.0.2.1/api";
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
            Self::new_for_test_with_creds(MockCreds::new().creds())
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
                publisher: MockPublisher::new().publisher(),
                creds,
                gateway,
                health_tx,
                health_state,
                content_state: make_content_state(),
                rate_limiter,
                webhook_registry_serial: tokio::sync::Mutex::new(()),
            })
        }
    }

    pub(crate) struct MockPublisher {
        pub events: Arc<Mutex<Vec<Event>>>,
    }

    impl MockPublisher {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self {
                events: Arc::new(Mutex::new(Vec::new())),
            })
        }

        pub(crate) fn publisher(self: &Arc<Self>) -> Arc<dyn EventPublisher> {
            Arc::clone(self) as Arc<dyn EventPublisher>
        }

        pub(crate) fn has_kind(&self, kind: &str) -> bool {
            self.events.lock().unwrap().iter().any(|e| e.kind == kind)
        }

        pub(crate) fn find_kind(&self, kind: &str) -> Option<Event> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.kind == kind)
                .cloned()
        }

        fn rendered(&self) -> Vec<String> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .map(|e| e.payload.to_string())
                .collect()
        }
    }

    impl EventPublisher for MockPublisher {
        fn publish(&self, event: Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    pub(crate) struct MockCreds {
        store: Arc<Mutex<HashMap<String, String>>>,
    }

    impl MockCreds {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self {
                store: Arc::new(Mutex::new(HashMap::new())),
            })
        }

        pub(crate) fn creds(self: &Arc<Self>) -> Arc<dyn CredentialsRepo> {
            Arc::clone(self) as Arc<dyn CredentialsRepo>
        }

        pub(crate) fn insert(&self, key: &str, value: &str) {
            self.store
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
        }

        pub(crate) fn peek(&self, key: &str) -> Option<String> {
            self.store.lock().unwrap().get(key).cloned()
        }

        pub(crate) fn keys(&self) -> Vec<String> {
            let mut keys: Vec<String> = self.store.lock().unwrap().keys().cloned().collect();
            keys.sort();
            keys
        }

        fn store_hook(&self, name: &str, url: &str) {
            self.insert(
                &format!("{DISCORD_CRED_PREFIX}{name}"),
                &serde_json::json!({ "url": url }).to_string(),
            );
        }
    }

    #[async_trait]
    impl CredentialsRepo for MockCreds {
        async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
            self.store
                .lock()
                .unwrap()
                .insert(id.as_str().to_owned(), plaintext.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.store.lock().unwrap().get(id.as_str()).cloned())
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.store.lock().unwrap().remove(id.as_str()).is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self
                .store
                .lock()
                .unwrap()
                .keys()
                .map(|k| CredentialId::new(k.clone()))
                .collect())
        }

        async fn last_refresh(
            &self,
            _id: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    fn with_bucket_headers(template: ResponseTemplate, limit: &str) -> ResponseTemplate {
        template
            .insert_header("x-ratelimit-limit", limit)
            .insert_header("x-ratelimit-remaining", "4")
            .insert_header("x-ratelimit-reset-after", "1.0")
    }

    fn make_standard_response(message_id: &str) -> ResponseTemplate {
        with_bucket_headers(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": message_id })),
            "5",
        )
    }

    fn make_429_response(retry_after_secs: f64) -> ResponseTemplate {
        ResponseTemplate::new(429)
            .set_body_json(serde_json::json!({ "retry_after": retry_after_secs }))
            .insert_header("retry-after", retry_after_secs.to_string().as_str())
    }

    fn client_at(
        api_base: &str,
        publisher: &Arc<MockPublisher>,
        creds: &Arc<MockCreds>,
    ) -> Arc<DiscordClient> {
        DiscordClient::new(api_base, publisher.publisher(), creds.creds()).unwrap()
    }

    async fn make_client(
        server: &MockServer,
        pub_ref: Arc<MockPublisher>,
        creds_ref: Arc<MockCreds>,
    ) -> Arc<DiscordClient> {
        creds_ref.store_hook("alerts", &hook_url("discord.com", ""));
        client_at(&server.uri(), &pub_ref, &creds_ref)
    }

    async fn received_count(server: &MockServer) -> usize {
        server.received_requests().await.unwrap().len()
    }

    #[tokio::test]
    async fn post_text_happy_path_returns_message_id() {
        let server = MockServer::start().await;
        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .and(query_param("wait", "true"))
            .respond_with(make_standard_response("msg001"))
            .mount(&server)
            .await;

        let result = client.post_text("alerts", "hello world").await;
        assert_eq!(result.unwrap(), "msg001");
    }

    #[tokio::test]
    async fn post_text_emits_posted_event() {
        let server = MockServer::start().await;
        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .and(query_param("wait", "true"))
            .respond_with(make_standard_response("msg002"))
            .mount(&server)
            .await;

        client.post_text("alerts", "hello").await.unwrap();

        let ev = publisher.find_kind("discord.webhook.posted").unwrap();
        assert_eq!(ev.payload["webhook_name"], "alerts");
        assert_eq!(ev.payload["message_id"], "msg002");
        assert_eq!(ev.payload["embed_count"], 0);
    }

    #[tokio::test]
    async fn post_embed_returns_message_id_with_embed_count_one() {
        let server = MockServer::start().await;
        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
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
        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
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

        let result = client.post_text("alerts", "retry test").await;
        assert_eq!(result.unwrap(), "msg_retry");
        assert_eq!(received_count(&server).await, 2);
        assert!(publisher.has_kind("discord.webhook.rate_limited"));
        assert!(publisher.has_kind("discord.webhook.posted"));
    }

    #[tokio::test]
    async fn double_429_returns_rate_limited_error() {
        let server = MockServer::start().await;
        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .respond_with(make_429_response(SHORT_HINT_SECS))
            .mount(&server)
            .await;

        let result = client.post_text("alerts", "double retry").await;
        assert!(matches!(result, Err(DiscordError::RateLimited { .. })));
        assert_eq!(received_count(&server).await, 2);
        assert!(publisher.has_kind("discord.webhook.failed"));
    }

    #[tokio::test]
    async fn a_second_consecutive_429_holds_the_webhook_until_its_hint_runs_out() {
        let server = MockServer::start().await;
        let client = make_client(&server, MockPublisher::new(), MockCreds::new()).await;
        Mock::given(method("POST"))
            .respond_with(make_429_response(SHORT_HINT_SECS))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(make_429_response(OVER_CAP_HINT_SECS))
            .mount(&server)
            .await;

        client.post_text("alerts", "first").await.unwrap_err();
        let held = client.post_text("alerts", "second").await;

        assert!(matches!(held, Err(DiscordError::RateLimited { .. })));
        assert_eq!(received_count(&server).await, 2);
    }

    #[tokio::test]
    async fn a_hint_over_the_wait_cap_fails_at_once_without_a_retry() {
        let server = MockServer::start().await;
        let publisher = MockPublisher::new();
        let client = make_client(&server, Arc::clone(&publisher), MockCreds::new()).await;
        Mock::given(method("POST"))
            .respond_with(make_429_response(OVER_CAP_HINT_SECS))
            .mount(&server)
            .await;

        let result = client.post_text("alerts", "hi").await;

        assert!(
            matches!(result, Err(DiscordError::RateLimited { retry_after_secs }) if retry_after_secs == OVER_CAP_HINT_SECS),
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
            let creds = MockCreds::new();
            creds.store_hook("clips", &hook_url("discord.com", ""));
            let client = make_client(&server, MockPublisher::new(), Arc::clone(&creds)).await;
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

            client.post_text("alerts", "throttled").await.unwrap_err();
            let same = client.post_text("alerts", "again").await;
            let other = client.post_text("clips", "elsewhere").await;

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
        let publisher = MockPublisher::new();
        let client = make_client(&server, Arc::clone(&publisher), MockCreds::new()).await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(503)
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
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let throttled = client.post_text("alerts", "hinted").await;
        let bare = client.post_text("alerts", "bare").await;

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

    #[tokio::test(start_paused = true)]
    async fn a_hint_of_exactly_the_wait_cap_is_waited_out_and_retried() {
        for (hint, retried) in [("10", true), ("10.5", false)] {
            let stub = scripted_http(vec![
                http_reply("429 Too Many Requests", &[("retry-after", hint)], "{}"),
                http_reply("200 OK", &[], r#"{"id":"msg_cap"}"#),
            ])
            .await;
            let creds = MockCreds::new();
            creds.store_hook("alerts", &hook_url("discord.com", ""));
            let client = client_at(&stub.base, &MockPublisher::new(), &creds);

            let result = client.post_text("alerts", "at the cap").await;

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
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": "m" })),
                    "0",
                ),
                (0, 0),
            ),
            (
                with_bucket_headers(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": "m" })),
                    "five",
                ),
                (0, 0),
            ),
            (with_bucket_headers(ResponseTemplate::new(404), "5"), (0, 0)),
            (with_bucket_headers(ResponseTemplate::new(500), "5"), (0, 0)),
        ] {
            let server = MockServer::start().await;
            let client = make_client(&server, MockPublisher::new(), MockCreds::new()).await;
            Mock::given(method("POST"))
                .respond_with(reply)
                .mount(&server)
                .await;

            let outcome = client.post_text("alerts", "bucket test").await;

            let budget = client.rate_limiter.lock().unwrap().budget("alerts");
            assert_eq!(budget, expected_budget, "{outcome:?}");
        }
    }

    #[tokio::test]
    async fn http_404_returns_bad_response_error_and_emits_failed() {
        let server = MockServer::start().await;
        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "code": 10015,
                "message": "Unknown Webhook"
            })))
            .mount(&server)
            .await;

        let result = client.post_text("alerts", "test").await;
        assert!(matches!(
            result,
            Err(DiscordError::BadResponse { status: 404, .. })
        ));
        assert!(publisher.has_kind("discord.webhook.failed"));
        assert!(!publisher.has_kind("request.fail"));
    }

    #[tokio::test]
    async fn failed_event_payload_carries_reason_status_and_detail() {
        let server = MockServer::start().await;
        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("POST"))
            .and(path(hook_path()))
            .respond_with(ResponseTemplate::new(404).set_body_string("Unknown Webhook"))
            .mount(&server)
            .await;

        client.post_text("alerts", "test").await.unwrap_err();

        let ev = publisher.find_kind("discord.webhook.failed").unwrap();
        assert_eq!(ev.payload["reason"], "http_status");
        assert_eq!(ev.payload["status_code"], 404);
        assert_eq!(ev.payload["detail"], "Unknown Webhook");
    }

    #[tokio::test]
    async fn an_error_body_echoing_the_webhook_id_or_token_never_surfaces_either() {
        let server = MockServer::start().await;
        let publisher = MockPublisher::new();
        let client = make_client(&server, Arc::clone(&publisher), MockCreds::new()).await;
        Mock::given(wiremock::matchers::any())
            .respond_with(ResponseTemplate::new(404).set_body_string(format!(
                r#"{{"message":"Unknown Webhook {HOOK_ID}/{HOOK_TOKEN}","code":10015}}"#
            )))
            .mount(&server)
            .await;

        let errors = [
            client.post_text("alerts", "hi").await.unwrap_err(),
            client
                .edit_message("alerts", MESSAGE_ID, Some("hi"), None)
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

        let mut surfaced = publisher.rendered();
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
        let creds = MockCreds::new();
        creds.store_hook("alerts", &hook_url("discord.com", ""));
        let override_base = format!("{}/api", server.uri());
        let endpoints = forge_platform_core::PlatformEndpoints::resolve(|name| {
            (name == forge_platform_core::EndpointSurface::DiscordApi.env_var())
                .then(|| override_base.clone().into())
        })
        .unwrap();
        let client = client_at(
            endpoints.base_url(forge_platform_core::EndpointSurface::DiscordApi),
            &MockPublisher::new(),
            &creds,
        );
        mount_accepting(&server, "POST").await;

        client.post_text("alerts", "hi").await.unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests[0].url.path(), format!("/api{}", hook_path()));
    }

    #[tokio::test]
    async fn rate_limited_event_payload_carries_webhook_name_and_retry_after() {
        let server = MockServer::start().await;
        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
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

        client.post_text("alerts", "rl test").await.unwrap();

        let ev = publisher.find_kind("discord.webhook.rate_limited").unwrap();
        assert_eq!(ev.payload["webhook_name"], "alerts");
        let ra = ev.payload["retry_after_secs"].as_f64().unwrap();
        assert!((ra - SHORT_HINT_SECS).abs() < 1e-9);
    }

    #[tokio::test]
    async fn missing_credential_returns_webhook_not_found() {
        let client = DiscordClient::new_for_test();
        let result = client.post_text("nonexistent", "hello").await;
        assert!(matches!(result, Err(DiscordError::WebhookNotFound { .. })));
    }

    #[tokio::test]
    async fn embed_with_invalid_content_rejected_before_http() {
        let server = MockServer::start().await;
        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        let embed = DiscordEmbed {
            title: Some("bad\0null".to_owned()),
            ..Default::default()
        };
        let result = client.post_embed("alerts", embed).await;
        assert!(matches!(result, Err(DiscordError::Validation(_))));
        assert_eq!(received_count(&server).await, 0);
    }

    #[tokio::test]
    async fn edit_message_patch_succeeds() {
        let server = MockServer::start().await;
        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let client = make_client(&server, Arc::clone(&publisher), Arc::clone(&creds)).await;

        Mock::given(method("PATCH"))
            .and(path(message_path()))
            .respond_with(make_standard_response(MESSAGE_ID))
            .mount(&server)
            .await;

        let result = client
            .edit_message("alerts", MESSAGE_ID, Some("updated"), None)
            .await;
        assert!(result.is_ok(), "{result:?}");
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
            let creds = MockCreds::new();
            creds.store_hook(
                "alerts",
                &format!("https://discord.com/api/webhooks/{id}/{token}"),
            );
            let client = client_at(&server.uri(), &MockPublisher::new(), &creds);

            let err = client.post_text("alerts", "hi").await.unwrap_err();

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
        let client = make_client(&server, MockPublisher::new(), MockCreds::new()).await;
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
            let edited = client.edit_message("alerts", bad, Some("x"), None).await;
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
            let creds = MockCreds::new();
            creds.store_hook("alerts", &hook_url(host, "?wait=false&thread_id=987"));
            let client = client_at(&server.uri(), &MockPublisher::new(), &creds);
            mount_accepting(&server, "POST").await;

            client.post_text("alerts", "hi").await.unwrap();

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
        let creds = MockCreds::new();
        creds.store_hook("alerts", &hook_url("discordapp.com", "?thread_id=987"));
        let client = client_at(&server.uri(), &MockPublisher::new(), &creds);
        Mock::given(wiremock::matchers::any())
            .respond_with(make_standard_response(MESSAGE_ID))
            .mount(&server)
            .await;

        client
            .edit_message("alerts", MESSAGE_ID, Some("edited"), None)
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

    #[tokio::test(start_paused = true)]
    async fn transport_failures_surface_as_connect_without_the_endpoint() {
        let creds = MockCreds::new();
        creds.store_hook("leaky", &hook_url("discord.com", ""));
        let client = client_at(UNROUTABLE_API_BASE, &MockPublisher::new(), &creds);
        let embed = DiscordEmbed {
            title: Some("t".to_owned()),
            ..Default::default()
        };

        let errors = [
            client
                .post_text_with_mentions("leaky", "hi", MentionPolicy::default())
                .await
                .unwrap_err(),
            client.post_embed("leaky", embed).await.unwrap_err(),
            client
                .edit_message_with_mentions(
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

    #[tokio::test(start_paused = true)]
    async fn a_transport_failure_writes_no_health_and_publishes_no_failed_event() {
        let creds = MockCreds::new();
        creds.store_hook("alerts", &hook_url("discord.com", ""));
        let publisher = MockPublisher::new();
        let client = client_at(UNROUTABLE_API_BASE, &publisher, &creds);

        client.post_text("alerts", "hi").await.unwrap_err();

        let health = client.send_health();
        assert_eq!(health.latency_samples, 0);
        assert_eq!(health.last_send_ok, None);
        assert!(!publisher.has_kind("discord.webhook.failed"));
        assert!(!publisher.has_kind("request.fail"));
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
        let client = make_client(&server, MockPublisher::new(), MockCreds::new()).await;
        mount_accepting(&server, "POST").await;

        for (policy, _) in mention_cases() {
            client
                .post_text_with_mentions("alerts", "<@&42> @everyone live", policy)
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
        let client = make_client(&server, MockPublisher::new(), MockCreds::new()).await;
        mount_accepting(&server, "POST").await;

        client.post_text("alerts", "@everyone test").await.unwrap();

        let bodies = request_bodies(&server).await;
        assert_eq!(
            bodies[0]["allowed_mentions"],
            serde_json::json!({ "parse": ["users", "roles"] })
        );
    }

    #[tokio::test]
    async fn edit_message_sends_the_allowed_mentions_of_each_toggle_combination() {
        let server = MockServer::start().await;
        let client = make_client(&server, MockPublisher::new(), MockCreds::new()).await;
        mount_accepting(&server, "PATCH").await;

        for (policy, _) in mention_cases() {
            client
                .edit_message_with_mentions("alerts", MESSAGE_ID, Some("<@&42> live"), None, policy)
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
        let client = make_client(&server, MockPublisher::new(), MockCreds::new()).await;
        mount_accepting(&server, "PATCH").await;
        let embed = DiscordEmbed {
            title: Some("live".to_owned()),
            ..Default::default()
        };

        client
            .edit_message_with_mentions(
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
        let client = make_client(&server, MockPublisher::new(), MockCreds::new()).await;
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
        let client = make_client(&server, MockPublisher::new(), MockCreds::new()).await;
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
        let client = make_client(&server, MockPublisher::new(), MockCreds::new()).await;
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
        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
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
