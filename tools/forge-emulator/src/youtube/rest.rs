use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};
use serde_json::{Map, Value, json};

use super::config::{FakeYouTubeConfig, YouTubeRefreshAnswer};
use super::ledger::{YouTubeCredentialCheck, YouTubeRequest, YouTubeSurface};
use super::state::{Inner, Shared};
use crate::fixture::REDACTED;

pub(crate) const DATA_API_PREFIX: &str = "/youtube/v3";
pub(crate) const UPLOAD_API_PREFIX: &str = "/upload/youtube/v3";
pub(crate) const OAUTH_PREFIX: &str = "/oauth2";

pub const LIVE_CHAT_MESSAGES_PATH: &str = "/youtube/v3/liveChat/messages";
pub const LIVE_BROADCASTS_PATH: &str = "/youtube/v3/liveBroadcasts";
pub const TOKEN_PATH: &str = "/oauth2/token";

const REFRESH_GRANT: &str = "refresh_token";
const TOKEN_LIFETIME_SECS: u64 = 3599;
const TOKEN_SCOPE: &str = "https://www.googleapis.com/auth/youtube.force-ssl";
const PAGE_TOKEN_PREFIX: &str = "emulator-page-";
const LIVE_CHAT_DOMAIN: &str = "youtube.liveChat";
const GLOBAL_DOMAIN: &str = "global";
const INVALID_CREDENTIALS: &str = "Request had invalid authentication credentials. Expected OAuth 2 access token, login cookie or other valid authentication credential. See https://developers.google.com/identity/sign-in/web/devconsole-project.";
const UNMODELED_INSERT: &str = "The fake YouTube models only textMessageEvent inserts.";
const REVOKED_GRANT: &str = "Token has been expired or revoked.";

#[derive(Debug, Clone, Copy)]
enum Route {
    ListChat,
    InsertChat,
    ListBroadcasts,
    ListVideos,
    ListChannels,
    Token,
}

fn locate(method: &Method, path: &str) -> (YouTubeSurface, Option<Route>) {
    if path.starts_with(UPLOAD_API_PREFIX) {
        return (YouTubeSurface::UploadApi, None);
    }
    if let Some(rest) = path.strip_prefix(DATA_API_PREFIX) {
        let route = match (method.as_str(), rest) {
            ("GET", "/liveChat/messages") => Some(Route::ListChat),
            ("POST", "/liveChat/messages") => Some(Route::InsertChat),
            ("GET", "/liveBroadcasts") => Some(Route::ListBroadcasts),
            ("GET", "/videos") => Some(Route::ListVideos),
            ("GET", "/channels") => Some(Route::ListChannels),
            _ => None,
        };
        return (YouTubeSurface::DataApi, route);
    }
    if let Some(rest) = path.strip_prefix(OAUTH_PREFIX) {
        let route = (method.as_str() == "POST" && rest == "/token").then_some(Route::Token);
        return (YouTubeSurface::OAuth, route);
    }
    (YouTubeSurface::Unknown, None)
}

pub(crate) fn router(shared: Arc<Shared>) -> Router {
    Router::new().fallback(handle).with_state(shared)
}

async fn handle(
    State(shared): State<Arc<Shared>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let query = Query::<Vec<(String, String)>>::try_from_uri(&uri)
        .map(|Query(pairs)| pairs)
        .unwrap_or_default();
    let (surface, route) = locate(&method, uri.path());
    let body = decode_body(surface, &body);
    let (status, response) = shared.mutate(|inner| {
        let credentials = match surface {
            YouTubeSurface::DataApi | YouTubeSurface::UploadApi => {
                check_bearer(&headers, &inner.tokens.access)
            }
            YouTubeSurface::OAuth | YouTubeSurface::Unknown => YouTubeCredentialCheck::NotRequired,
        };
        let (status, response) = match route {
            None => google_error(
                StatusCode::NOT_FOUND,
                GLOBAL_DOMAIN,
                "notFound",
                "Not Found",
            ),
            Some(_)
                if !matches!(
                    credentials,
                    YouTubeCredentialCheck::Accepted | YouTubeCredentialCheck::NotRequired
                ) =>
            {
                unauthenticated()
            }
            Some(route) => serve(inner, shared.config(), route, &query, body.as_ref()),
        };
        inner.record_request(YouTubeRequest {
            surface,
            method: method.as_str().to_owned(),
            path: uri.path().to_owned(),
            query: query.clone(),
            body: body.clone(),
            credentials,
            status: status.as_u16(),
            response: response.clone(),
            modeled: route.is_some() && !unmodeled_insert(&response),
        });
        (status, response)
    });
    (status, Json(response)).into_response()
}

fn unmodeled_insert(response: &Value) -> bool {
    response.pointer("/error/message").and_then(Value::as_str) == Some(UNMODELED_INSERT)
}

fn decode_body(surface: YouTubeSurface, bytes: &Bytes) -> Option<Value> {
    if bytes.is_empty() {
        return None;
    }
    if surface == YouTubeSurface::OAuth
        && let Ok(pairs) = serde_urlencoded::from_bytes::<Vec<(String, String)>>(bytes)
    {
        let form: Map<String, Value> = pairs
            .into_iter()
            .map(|(key, value)| {
                let value = if key == "client_secret" {
                    REDACTED.to_owned()
                } else {
                    value
                };
                (key, Value::String(value))
            })
            .collect();
        return Some(Value::Object(form));
    }
    if surface == YouTubeSurface::UploadApi {
        return Some(json!({ "bytes": bytes.len() }));
    }
    Some(
        serde_json::from_slice(bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned())),
    )
}

fn check_bearer(headers: &HeaderMap, access_token: &str) -> YouTubeCredentialCheck {
    let bearer = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    match bearer {
        None => YouTubeCredentialCheck::MissingBearer,
        Some(token) if token != access_token => YouTubeCredentialCheck::WrongBearer,
        Some(_) => YouTubeCredentialCheck::Accepted,
    }
}

fn google_error(
    status: StatusCode,
    domain: &str,
    reason: &str,
    message: &str,
) -> (StatusCode, Value) {
    (
        status,
        json!({
            "error": {
                "code": status.as_u16(),
                "message": message,
                "errors": [{ "message": message, "domain": domain, "reason": reason }]
            }
        }),
    )
}

fn unauthenticated() -> (StatusCode, Value) {
    (
        StatusCode::UNAUTHORIZED,
        json!({
            "error": {
                "code": StatusCode::UNAUTHORIZED.as_u16(),
                "message": INVALID_CREDENTIALS,
                "errors": [{
                    "message": "Invalid Credentials",
                    "domain": GLOBAL_DOMAIN,
                    "reason": "authError",
                    "location": "Authorization",
                    "locationType": "header"
                }],
                "status": "UNAUTHENTICATED"
            }
        }),
    )
}

fn query_value<'a>(query: &'a [(String, String)], key: &str) -> Option<&'a str> {
    query
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

fn parts(query: &[(String, String)]) -> Vec<&str> {
    query_value(query, "part")
        .map(|part| part.split(',').map(str::trim).collect())
        .unwrap_or_default()
}

fn with_parts(base: Map<String, Value>, wanted: &[&str], available: Vec<(&str, Value)>) -> Value {
    let mut resource = base;
    for (part, value) in available {
        if wanted.contains(&part) {
            resource.insert(part.to_owned(), value);
        }
    }
    Value::Object(resource)
}

fn list(kind: &str, items: Vec<Value>) -> Value {
    json!({
        "kind": kind,
        "etag": "emulator-list-etag",
        "pageInfo": { "totalResults": items.len(), "resultsPerPage": items.len() },
        "items": items
    })
}

fn serve(
    inner: &mut Inner,
    config: &FakeYouTubeConfig,
    route: Route,
    query: &[(String, String)],
    body: Option<&Value>,
) -> (StatusCode, Value) {
    match route {
        Route::ListChat => list_chat(inner, config, query),
        Route::InsertChat => insert_chat(inner, config, body),
        Route::ListBroadcasts => (
            StatusCode::OK,
            list(
                "youtube#liveBroadcastListResponse",
                broadcasts(inner, config, query),
            ),
        ),
        Route::ListVideos => (
            StatusCode::OK,
            list("youtube#videoListResponse", videos(inner, config, query)),
        ),
        Route::ListChannels => (
            StatusCode::OK,
            list("youtube#channelListResponse", channels(config, query)),
        ),
        Route::Token => refresh(inner, config, body),
    }
}

fn list_chat(
    inner: &Inner,
    config: &FakeYouTubeConfig,
    query: &[(String, String)],
) -> (StatusCode, Value) {
    if query_value(query, "liveChatId") != Some(config.live_chat_id.as_str()) {
        return google_error(
            StatusCode::NOT_FOUND,
            LIVE_CHAT_DOMAIN,
            "liveChatNotFound",
            "The live chat that you are trying to retrieve cannot be found.",
        );
    }
    let offset = match query_value(query, "pageToken") {
        None => 0,
        Some(token) => match token
            .strip_prefix(PAGE_TOKEN_PREFIX)
            .and_then(|offset| offset.parse::<usize>().ok())
            .filter(|offset| *offset <= inner.chat.len())
        {
            Some(offset) => offset,
            None => {
                return google_error(
                    StatusCode::BAD_REQUEST,
                    GLOBAL_DOMAIN,
                    "badRequest",
                    "The page token is not one this chat issued.",
                );
            }
        },
    };
    let ended_and_delivered = inner
        .broadcast
        .ended_at_message
        .is_some_and(|end| offset >= end);
    let never_live = !inner.chat_open() && inner.broadcast.ended_at_message.is_none();
    if ended_and_delivered || never_live {
        return google_error(
            StatusCode::FORBIDDEN,
            LIVE_CHAT_DOMAIN,
            "liveChatEnded",
            "The live chat is no longer live.",
        );
    }
    let items = inner.chat[offset..].to_vec();
    let mut response = json!({
        "kind": "youtube#liveChatMessageListResponse",
        "etag": format!("emulator-chat-etag-{}", inner.chat.len()),
        "nextPageToken": format!("{PAGE_TOKEN_PREFIX}{}", inner.chat.len()),
        "pollingIntervalMillis": config.polling_interval_ms,
        "pageInfo": { "totalResults": items.len(), "resultsPerPage": items.len() },
        "items": items
    });
    if let Some(offline_at) = &inner.broadcast.offline_at {
        response["offlineAt"] = json!(offline_at);
    }
    (StatusCode::OK, response)
}

fn insert_chat(
    inner: &mut Inner,
    config: &FakeYouTubeConfig,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let field = |pointer: &str| body.and_then(|body| body.pointer(pointer));
    let invalid = |reason: &str, message: &str| {
        google_error(StatusCode::BAD_REQUEST, LIVE_CHAT_DOMAIN, reason, message)
    };
    let Some(live_chat_id) = field("/snippet/liveChatId").and_then(Value::as_str) else {
        return invalid("liveChatIdRequired", "The liveChatId is required.");
    };
    if live_chat_id != config.live_chat_id {
        return google_error(
            StatusCode::NOT_FOUND,
            LIVE_CHAT_DOMAIN,
            "liveChatNotFound",
            "The live chat that you are trying to insert a message into cannot be found.",
        );
    }
    match field("/snippet/type").and_then(Value::as_str) {
        None => return invalid("typeRequired", "The type is required."),
        Some("textMessageEvent") => {}
        Some(_) => {
            return google_error(
                StatusCode::BAD_REQUEST,
                GLOBAL_DOMAIN,
                "badRequest",
                UNMODELED_INSERT,
            );
        }
    }
    let text = field("/snippet/textMessageDetails/messageText")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if text.trim().is_empty() {
        return invalid("messageTextRequired", "The messageText is required.");
    }
    if !inner.chat_open() {
        return google_error(
            StatusCode::FORBIDDEN,
            LIVE_CHAT_DOMAIN,
            "liveChatEnded",
            "The live chat is no longer live.",
        );
    }
    let mut snippet = Map::new();
    snippet.insert("type".to_owned(), json!("textMessageEvent"));
    snippet.insert("displayMessage".to_owned(), json!(text));
    snippet.insert(
        "textMessageDetails".to_owned(),
        json!({ "messageText": text }),
    );
    inner.append_message(config, snippet, Inner::owner_details(config));
    let mut created = inner.chat.last().cloned().unwrap_or(Value::Null);
    if let Some(resource) = created.as_object_mut() {
        resource.remove("authorDetails");
    }
    (StatusCode::OK, created)
}

fn broadcasts(inner: &Inner, config: &FakeYouTubeConfig, query: &[(String, String)]) -> Vec<Value> {
    let asks_for_active = query_value(query, "broadcastStatus") == Some("active");
    let asks_for_own = query_value(query, "mine") == Some("true")
        || query_value(query, "id") == Some(config.broadcast_id.as_str());
    if !inner.broadcast.live || !(asks_for_active || asks_for_own) {
        return Vec::new();
    }
    let started_at = inner.broadcast.started_at.clone().unwrap_or_default();
    let mut base = Map::new();
    base.insert("kind".to_owned(), json!("youtube#liveBroadcast"));
    base.insert("etag".to_owned(), json!("emulator-broadcast-etag"));
    base.insert("id".to_owned(), json!(config.broadcast_id));
    vec![with_parts(
        base,
        &parts(query),
        vec![
            (
                "snippet",
                json!({
                    "publishedAt": started_at,
                    "channelId": config.channel_id,
                    "title": config.broadcast_title,
                    "description": "",
                    "scheduledStartTime": started_at,
                    "actualStartTime": started_at,
                    "isDefaultBroadcast": false,
                    "liveChatId": config.live_chat_id
                }),
            ),
            (
                "status",
                json!({
                    "lifeCycleStatus": "live",
                    "privacyStatus": "public",
                    "recordingStatus": "recording"
                }),
            ),
            (
                "contentDetails",
                json!({ "enableDvr": true, "latencyPreference": "normal" }),
            ),
        ],
    )]
}

fn videos(inner: &Inner, config: &FakeYouTubeConfig, query: &[(String, String)]) -> Vec<Value> {
    let asked = query_value(query, "id")
        .map(|ids| ids.split(',').any(|id| id == config.broadcast_id))
        .unwrap_or(false);
    if !asked || inner.broadcast.started_at.is_none() {
        return Vec::new();
    }
    let started_at = inner.broadcast.started_at.clone().unwrap_or_default();
    let mut streaming = json!({
        "actualStartTime": started_at,
        "scheduledStartTime": started_at
    });
    if inner.broadcast.live {
        streaming["concurrentViewers"] = json!(config.concurrent_viewers.to_string());
        streaming["activeLiveChatId"] = json!(config.live_chat_id);
    }
    if let Some(offline_at) = &inner.broadcast.offline_at {
        streaming["actualEndTime"] = json!(offline_at);
    }
    let mut base = Map::new();
    base.insert("kind".to_owned(), json!("youtube#video"));
    base.insert("etag".to_owned(), json!("emulator-video-etag"));
    base.insert("id".to_owned(), json!(config.broadcast_id));
    vec![with_parts(
        base,
        &parts(query),
        vec![
            (
                "snippet",
                json!({
                    "publishedAt": started_at,
                    "channelId": config.channel_id,
                    "title": config.broadcast_title,
                    "description": "",
                    "channelTitle": config.channel_title,
                    "categoryId": "20",
                    "liveBroadcastContent": if inner.broadcast.live { "live" } else { "none" }
                }),
            ),
            (
                "status",
                json!({ "uploadStatus": "uploaded", "privacyStatus": "public" }),
            ),
            (
                "statistics",
                json!({ "viewCount": "0", "likeCount": "0", "commentCount": "0" }),
            ),
            ("liveStreamingDetails", streaming),
        ],
    )]
}

fn channels(config: &FakeYouTubeConfig, query: &[(String, String)]) -> Vec<Value> {
    let handle = config.channel_handle.trim_start_matches('@');
    let own = query_value(query, "mine") == Some("true")
        || query_value(query, "id") == Some(config.channel_id.as_str())
        || query_value(query, "forHandle")
            .is_some_and(|asked| !handle.is_empty() && asked.trim_start_matches('@') == handle);
    if !own {
        return Vec::new();
    }
    let mut snippet = json!({
        "title": config.channel_title,
        "description": "",
        "publishedAt": "2020-01-01T00:00:00Z"
    });
    if !config.channel_handle.is_empty() {
        snippet["customUrl"] = json!(config.channel_handle);
    }
    let mut base = Map::new();
    base.insert("kind".to_owned(), json!("youtube#channel"));
    base.insert("etag".to_owned(), json!("emulator-channel-etag"));
    base.insert("id".to_owned(), json!(config.channel_id));
    vec![with_parts(
        base,
        &parts(query),
        vec![
            ("snippet", snippet),
            (
                "statistics",
                json!({
                    "viewCount": "0",
                    "subscriberCount": "0",
                    "hiddenSubscriberCount": false,
                    "videoCount": "0"
                }),
            ),
        ],
    )]
}

fn refresh(
    inner: &mut Inner,
    config: &FakeYouTubeConfig,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let field = |name: &str| {
        body.and_then(|body| body.get(name))
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    if field("client_id").is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            json!({ "error": "invalid_request", "error_description": "Could not determine client ID from request." }),
        );
    }
    if field("grant_type") != REFRESH_GRANT {
        return (
            StatusCode::BAD_REQUEST,
            json!({ "error": "unsupported_grant_type", "error_description": format!("Invalid grant_type: {}", field("grant_type")) }),
        );
    }
    let accepted = config.refresh == YouTubeRefreshAnswer::Accept
        && field("refresh_token") == inner.tokens.refresh;
    if !accepted {
        return (
            StatusCode::BAD_REQUEST,
            json!({ "error": "invalid_grant", "error_description": REVOKED_GRANT }),
        );
    }
    let tokens = &mut inner.tokens;
    tokens.rotations += 1;
    tokens.access = format!("{}-refreshed-{}", config.access_token, tokens.rotations);
    (
        StatusCode::OK,
        json!({
            "access_token": tokens.access,
            "expires_in": TOKEN_LIFETIME_SECS,
            "scope": TOKEN_SCOPE,
            "token_type": "Bearer"
        }),
    )
}
