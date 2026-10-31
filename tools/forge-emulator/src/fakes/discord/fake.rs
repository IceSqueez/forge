use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Json, Response};
use forge_platform_core::{EndpointSurface, PlatformEndpoints};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::watch;

use super::multipart::{FormPart, boundary, form_parts};
use crate::EmulatorError;
use crate::fakes::support::{Observed, ServerTasks, bind_loopback, local_addr, serve_http};
use crate::fixture::Fixture;

pub const UNKNOWN_WEBHOOK_CODE: u64 = 10015;

const API_PATH: &str = "/api";
const WEBHOOK_PATH_PREFIX: &str = "/api/webhooks/";
const MESSAGES_SEGMENT: &str = "messages";
const WAIT_PARAMETER: &str = "wait";
const PAYLOAD_JSON_FIELD: &str = "payload_json";
const FIRST_WEBHOOK_ID: u64 = 1_300_000_000_000_000_001;
const FIRST_MESSAGE_ID: u64 = 1_400_000_000_000_000_001;
const CHANNEL_ID: &str = "1200000000000000001";
const RATE_LIMIT_HEADER: &str = "x-ratelimit-limit";
const RATE_REMAINING_HEADER: &str = "x-ratelimit-remaining";
const RATE_RESET_AFTER_HEADER: &str = "x-ratelimit-reset-after";
const BUCKET_LIMIT: &str = "5";
const BUCKET_REMAINING: &str = "4";
const BUCKET_RESET_AFTER: &str = "1.0";
const INVALID_JSON_CODE: u32 = 50_109;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedPost {
    pub webhook: Option<String>,
    pub method: String,
    pub message_id: Option<String>,
    pub wait: bool,
    pub content: Option<String>,
    pub embeds: Vec<Value>,
    pub allowed_mentions: Option<Value>,
    pub files: Vec<String>,
    pub body_error: Option<String>,
    pub status: u16,
}

impl RecordedPost {
    pub fn mention_parse(&self) -> Option<Vec<String>> {
        let parse = self.allowed_mentions.as_ref()?.get("parse")?.as_array()?;
        let mut kinds: Vec<String> = parse
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        kinds.sort();
        Some(kinds)
    }
}

struct Webhook {
    name: String,
    id: String,
    token: String,
}

struct Inner {
    webhooks: Vec<Webhook>,
    posts: Vec<RecordedPost>,
    rate_limits: VecDeque<f64>,
    next_message_id: u64,
}

type Shared = Observed<Inner>;

pub struct FakeDiscord {
    shared: Arc<Shared>,
    base_url: String,
    server: ServerTasks,
}

impl FakeDiscord {
    pub async fn start(webhook_names: &[String]) -> Result<Self, EmulatorError> {
        let mut webhooks: Vec<Webhook> = Vec::with_capacity(webhook_names.len());
        for (offset, name) in (0u64..).zip(webhook_names) {
            if name.trim().is_empty() || webhooks.iter().any(|known| &known.name == name) {
                return Err(EmulatorError::InvalidFakeConfig {
                    reason: format!("webhook name `{name}` is blank or declared twice"),
                });
            }
            webhooks.push(Webhook {
                name: name.clone(),
                id: (FIRST_WEBHOOK_ID + offset).to_string(),
                token: format!("fake-discord-token-{offset}"),
            });
        }
        let listener = bind_loopback().await?;
        let address: SocketAddr = local_addr(&listener)?;
        let shared = Arc::new(Observed::new(Inner {
            webhooks,
            posts: Vec::new(),
            rate_limits: VecDeque::new(),
            next_message_id: FIRST_MESSAGE_ID,
        }));
        let (shutdown, shutdown_rx) = watch::channel(false);
        let router = Router::new()
            .fallback(handle)
            .with_state(Arc::clone(&shared));
        let task = serve_http(listener, router, shutdown_rx);
        Ok(Self {
            shared,
            base_url: format!("http://{address}"),
            server: ServerTasks::new(shutdown, vec![task]),
        })
    }

    pub fn webhook_url(&self, name: &str) -> Option<String> {
        self.shared.read(|inner| {
            inner
                .webhooks
                .iter()
                .find(|webhook| webhook.name == name)
                .map(|webhook| {
                    format!(
                        "{}/webhooks/{}/{}",
                        PlatformEndpoints::default().base_url(EndpointSurface::DiscordApi),
                        webhook.id,
                        webhook.token
                    )
                })
        })
    }

    pub fn endpoint_override(&self) -> (&'static str, String) {
        (
            EndpointSurface::DiscordApi.env_var(),
            format!("{}{API_PATH}", self.base_url),
        )
    }

    pub fn addressed(&self, fixture: &Fixture) -> Fixture {
        let mut addressed = fixture.clone();
        for webhook in &mut addressed.discord_webhooks {
            if let Some(url) = self.webhook_url(&webhook.name) {
                webhook.url = url;
            }
        }
        addressed
    }

    pub fn rate_limit_next(&self, retry_after_secs: f64) {
        self.shared
            .mutate(|inner| inner.rate_limits.push_back(retry_after_secs));
    }

    pub fn posts(&self) -> Vec<RecordedPost> {
        self.shared.read(|inner| inner.posts.clone())
    }

    pub async fn wait_for<T>(
        &self,
        what: &str,
        timeout: Duration,
        mut probe: impl FnMut(&[RecordedPost]) -> Option<T>,
    ) -> Result<T, EmulatorError> {
        self.shared
            .wait_for(what, timeout, |inner| probe(&inner.posts))
            .await
    }

    pub async fn shutdown(self) {
        self.server.shutdown().await;
    }
}

struct Target {
    id: String,
    token: String,
    message_id: Option<String>,
}

fn target(path: &str) -> Option<Target> {
    let rest = path.strip_prefix(WEBHOOK_PATH_PREFIX)?;
    let segments: Vec<&str> = rest.split('/').collect();
    let message_id = match segments.as_slice() {
        [_, _] => None,
        [_, _, MESSAGES_SEGMENT, message_id] if !message_id.is_empty() => {
            Some((*message_id).to_owned())
        }
        _ => return None,
    };
    if segments[0].is_empty() || segments[1].is_empty() {
        return None;
    }
    Some(Target {
        id: segments[0].to_owned(),
        token: segments[1].to_owned(),
        message_id,
    })
}

#[derive(Default)]
struct Payload {
    content: Option<String>,
    embeds: Vec<Value>,
    allowed_mentions: Option<Value>,
    files: Vec<String>,
    body_error: Option<String>,
}

impl Payload {
    fn from_json(json: &Value) -> Self {
        Self {
            content: json
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_owned),
            embeds: json
                .get("embeds")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            allowed_mentions: json.get("allowed_mentions").cloned(),
            files: Vec::new(),
            body_error: None,
        }
    }

    fn from_bytes(json: &[u8]) -> Self {
        serde_json::from_slice::<Value>(json).map_or_else(
            |e| Self {
                body_error: Some(e.to_string()),
                ..Self::default()
            },
            |json| Self::from_json(&json),
        )
    }

    fn decode(headers: &HeaderMap, body: &[u8]) -> Self {
        let content_type = headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let Some(boundary) = boundary(content_type) else {
            return if body.is_empty() {
                Self::default()
            } else {
                Self::from_bytes(body)
            };
        };
        let parts = form_parts(boundary, body);
        let mut payload = parts
            .iter()
            .find(|part| part.name.as_deref() == Some(PAYLOAD_JSON_FIELD))
            .map(|part| Self::from_bytes(&part.data))
            .unwrap_or_default();
        payload.files = parts
            .iter()
            .filter_map(|part: &FormPart| part.file_name.clone())
            .collect();
        payload
    }
}

struct Answer {
    status: StatusCode,
    body: Option<Value>,
    retry_after: Option<f64>,
    message_id: Option<String>,
}

impl Answer {
    fn json(status: StatusCode, body: Value) -> Self {
        Self {
            status,
            body: Some(body),
            retry_after: None,
            message_id: None,
        }
    }

    fn empty() -> Self {
        Self {
            status: StatusCode::NO_CONTENT,
            body: None,
            retry_after: None,
            message_id: None,
        }
    }
}

async fn handle(
    State(shared): State<Arc<Shared>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let wait = Query::<Vec<(String, String)>>::try_from_uri(&uri)
        .map(|Query(pairs)| {
            pairs
                .iter()
                .any(|(key, value)| key == WAIT_PARAMETER && value == "true")
        })
        .unwrap_or(false);
    let target = target(uri.path());
    let payload = Payload::decode(&headers, &body);

    let answer = shared.mutate(|inner| {
        let webhook = target.as_ref().and_then(|target| {
            inner
                .webhooks
                .iter()
                .find(|webhook| webhook.id == target.id && webhook.token == target.token)
                .map(|webhook| (webhook.name.clone(), webhook.id.clone()))
        });
        let message_id = target.as_ref().and_then(|target| target.message_id.clone());
        let answer = answer(inner, &method, webhook.as_ref(), message_id, wait, &payload);
        inner.posts.push(RecordedPost {
            webhook: webhook.map(|(name, _)| name),
            method: method.as_str().to_owned(),
            message_id: answer.message_id.clone(),
            wait,
            content: payload.content.clone(),
            embeds: payload.embeds.clone(),
            allowed_mentions: payload.allowed_mentions.clone(),
            files: payload.files.clone(),
            body_error: payload.body_error.clone(),
            status: answer.status.as_u16(),
        });
        answer
    });
    respond(answer)
}

fn answer(
    inner: &mut Inner,
    method: &Method,
    webhook: Option<&(String, String)>,
    message_id: Option<String>,
    wait: bool,
    payload: &Payload,
) -> Answer {
    let Some((_, webhook_id)) = webhook else {
        return Answer::json(
            StatusCode::NOT_FOUND,
            json!({ "message": "Unknown Webhook", "code": UNKNOWN_WEBHOOK_CODE }),
        );
    };
    if let Some(retry_after) = inner.rate_limits.pop_front() {
        return Answer {
            retry_after: Some(retry_after),
            ..Answer::json(
                StatusCode::TOO_MANY_REQUESTS,
                json!({
                    "message": "You are being rate limited.",
                    "retry_after": retry_after,
                    "global": false
                }),
            )
        };
    }
    if payload.body_error.is_some() {
        return Answer::json(
            StatusCode::BAD_REQUEST,
            json!({
                "message": "The request body contains invalid JSON.",
                "code": INVALID_JSON_CODE
            }),
        );
    }
    let message = |id: &str| {
        json!({
            "id": id,
            "type": 0,
            "channel_id": CHANNEL_ID,
            "webhook_id": webhook_id,
            "content": payload.content.clone().unwrap_or_default(),
            "embeds": payload.embeds,
        })
    };
    match (method, message_id) {
        (&Method::POST, None) => {
            let id = inner.next_message_id.to_string();
            inner.next_message_id += 1;
            let mut answer = if wait {
                Answer::json(StatusCode::OK, message(&id))
            } else {
                Answer::empty()
            };
            answer.message_id = Some(id);
            answer
        }
        (&Method::PATCH, Some(id)) => Answer {
            message_id: Some(id.clone()),
            ..Answer::json(StatusCode::OK, message(&id))
        },
        (&Method::DELETE, Some(id)) => Answer {
            message_id: Some(id),
            ..Answer::empty()
        },
        _ => Answer::json(
            StatusCode::METHOD_NOT_ALLOWED,
            json!({ "message": "405: Method Not Allowed", "code": 0 }),
        ),
    }
}

fn respond(answer: Answer) -> Response {
    let mut response = match answer.body {
        Some(body) => (answer.status, Json(body)).into_response(),
        None => answer.status.into_response(),
    };
    let headers = response.headers_mut();
    if let Some(retry_after) = answer.retry_after {
        if let Ok(value) = retry_after.to_string().parse() {
            headers.insert(header::RETRY_AFTER, value);
        }
    } else if answer.status.is_success() {
        for (name, value) in [
            (RATE_LIMIT_HEADER, BUCKET_LIMIT),
            (RATE_REMAINING_HEADER, BUCKET_REMAINING),
            (RATE_RESET_AFTER_HEADER, BUCKET_RESET_AFTER),
        ] {
            headers.insert(name, axum::http::HeaderValue::from_static(value));
        }
    }
    response
}
