use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};
use serde_json::{Map, Value, json};

use super::config::{FakeKickConfig, RefreshAnswer};
use super::ledger::{KickCredentialCheck, KickRequest, KickSurface};
use super::state::{Inner, Shared};

pub(crate) const PUBLIC_API_PREFIX: &str = "/public/v1";
pub(crate) const CHANNEL_API_PREFIX: &str = "/api/v2";
pub(crate) const OAUTH_PREFIX: &str = "/oauth";

pub const SEND_CHAT_PATH: &str = "/public/v1/chat";
pub const CHANNELS_PATH: &str = "/public/v1/channels";
pub const TOKEN_PATH: &str = "/oauth/token";

const REFRESH_GRANT: &str = "refresh_token";
const TOKEN_LIFETIME_SECS: u64 = 3600;
const STREAM_STARTED_AT: &str = "2026-10-04T12:00:00Z";
const TOKEN_SCOPES: &str = "user:read channel:read channel:write channel:rewards:read channel:rewards:write chat:write moderation:chat_message:manage moderation:ban";
const OAUTH_REFUSAL: &str = "Invalid request";

#[derive(Debug, Clone)]
enum Route {
    SendChat,
    DeleteChat,
    Channels,
    PendingRedemptions,
    ChannelInfo(String),
    Token,
}

fn locate(method: &Method, path: &str) -> (KickSurface, Option<Route>) {
    if let Some(rest) = path.strip_prefix(PUBLIC_API_PREFIX) {
        let route = match (method.as_str(), rest) {
            ("POST", "/chat") => Some(Route::SendChat),
            ("GET", "/channels") => Some(Route::Channels),
            ("GET", "/channels/rewards/redemptions") => Some(Route::PendingRedemptions),
            ("DELETE", rest) => rest
                .strip_prefix("/chat/")
                .filter(|id| !id.is_empty() && !id.contains('/'))
                .map(|_| Route::DeleteChat),
            _ => None,
        };
        return (KickSurface::PublicApi, route);
    }
    if let Some(rest) = path.strip_prefix(CHANNEL_API_PREFIX) {
        let route = match (method.as_str(), rest.strip_prefix("/channels/")) {
            ("GET", Some(slug)) if !slug.is_empty() && !slug.contains('/') => {
                Some(Route::ChannelInfo(slug.to_owned()))
            }
            _ => None,
        };
        return (KickSurface::ChannelApi, route);
    }
    if let Some(rest) = path.strip_prefix(OAUTH_PREFIX) {
        let route = (method.as_str() == "POST" && rest == "/token").then_some(Route::Token);
        return (KickSurface::OAuth, route);
    }
    (KickSurface::Unknown, None)
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
            KickSurface::PublicApi => check_bearer(&headers, &inner.tokens.access),
            KickSurface::ChannelApi | KickSurface::OAuth | KickSurface::Unknown => {
                KickCredentialCheck::NotRequired
            }
        };
        let (status, response) = match &route {
            None => message(StatusCode::NOT_FOUND, "Not Found"),
            Some(_)
                if !matches!(
                    credentials,
                    KickCredentialCheck::Accepted | KickCredentialCheck::NotRequired
                ) =>
            {
                message(StatusCode::UNAUTHORIZED, "Unauthorized")
            }
            Some(route) => serve(inner, shared.config(), route, &query, body.as_ref()),
        };
        inner.record_request(KickRequest {
            surface,
            method: method.as_str().to_owned(),
            path: uri.path().to_owned(),
            query: query.clone(),
            body: body.clone(),
            credentials,
            status: status.as_u16(),
            response: response.clone(),
            modeled: route.is_some(),
        });
        (status, response)
    });
    if status == StatusCode::NO_CONTENT {
        return status.into_response();
    }
    (status, Json(response)).into_response()
}

fn decode_body(surface: KickSurface, bytes: &Bytes) -> Option<Value> {
    if bytes.is_empty() {
        return None;
    }
    if surface == KickSurface::OAuth
        && let Ok(pairs) = serde_urlencoded::from_bytes::<Vec<(String, String)>>(bytes)
    {
        let form: Map<String, Value> = pairs
            .into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect();
        return Some(Value::Object(form));
    }
    Some(
        serde_json::from_slice(bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned())),
    )
}

fn check_bearer(headers: &HeaderMap, access_token: &str) -> KickCredentialCheck {
    let bearer = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    match bearer {
        None => KickCredentialCheck::MissingBearer,
        Some(token) if token != access_token => KickCredentialCheck::WrongBearer,
        Some(_) => KickCredentialCheck::Accepted,
    }
}

fn message(status: StatusCode, text: &str) -> (StatusCode, Value) {
    (status, json!({ "message": text }))
}

fn serve(
    inner: &mut Inner,
    config: &FakeKickConfig,
    route: &Route,
    query: &[(String, String)],
    body: Option<&Value>,
) -> (StatusCode, Value) {
    match route {
        Route::SendChat => send_chat(config, body),
        Route::DeleteChat => (StatusCode::NO_CONTENT, Value::Null),
        Route::Channels => channels(inner, config, query),
        Route::PendingRedemptions => (StatusCode::OK, json!({ "data": [], "message": "OK" })),
        Route::ChannelInfo(slug) => channel_info(inner, config, slug),
        Route::Token => refresh(inner, config, body),
    }
}

fn send_chat(config: &FakeKickConfig, body: Option<&Value>) -> (StatusCode, Value) {
    let field = |name: &str| body.and_then(|body| body.get(name));
    let content = field("content").and_then(Value::as_str).unwrap_or_default();
    if content.trim().is_empty() {
        return message(StatusCode::BAD_REQUEST, "content is required");
    }
    match field("type").and_then(Value::as_str) {
        Some("bot") => {}
        Some("user") => {
            let broadcaster = field("broadcaster_user_id").and_then(Value::as_u64);
            if broadcaster != Some(config.user_id) {
                return message(
                    StatusCode::BAD_REQUEST,
                    "broadcaster_user_id must name the token's channel",
                );
            }
        }
        _ => return message(StatusCode::BAD_REQUEST, "type must be user or bot"),
    }
    (
        StatusCode::OK,
        json!({
            "data": { "is_sent": true, "message_id": super::fake::message_id() },
            "message": "OK"
        }),
    )
}

fn channels(
    inner: &Inner,
    config: &FakeKickConfig,
    query: &[(String, String)],
) -> (StatusCode, Value) {
    let foreign = query
        .iter()
        .any(|(key, value)| key == "slug" && *value != config.username);
    let data = if foreign {
        Vec::new()
    } else {
        vec![json!({
            "broadcaster_user_id": config.user_id,
            "slug": config.username,
            "channel_description": "",
            "banner_picture": "",
            "stream_title": config.stream_title,
            "category": {
                "id": config.category_id,
                "name": config.category_name,
                "thumbnail": ""
            },
            "stream": {
                "url": "",
                "key": "",
                "is_live": inner.channel.live,
                "is_mature": false,
                "language": "en",
                "start_time": if inner.channel.live { STREAM_STARTED_AT } else { "" },
                "viewer_count": if inner.channel.live { inner.channel.viewer_count } else { 0 },
                "thumbnail": ""
            }
        })]
    };
    (StatusCode::OK, json!({ "data": data, "message": "OK" }))
}

fn channel_info(inner: &Inner, config: &FakeKickConfig, slug: &str) -> (StatusCode, Value) {
    if slug != config.username {
        return message(StatusCode::NOT_FOUND, "Channel not found");
    }
    let livestream = if inner.channel.live {
        json!({
            "id": config.chatroom_id,
            "is_live": true,
            "session_title": config.stream_title,
            "viewer_count": inner.channel.viewer_count
        })
    } else {
        Value::Null
    };
    (
        StatusCode::OK,
        json!({
            "id": config.user_id,
            "user_id": config.user_id,
            "slug": config.username,
            "chatroom": { "id": config.chatroom_id, "channel_id": config.user_id },
            "livestream": livestream
        }),
    )
}

fn refresh(
    inner: &mut Inner,
    config: &FakeKickConfig,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let field = |name: &str| {
        body.and_then(|body| body.get(name))
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    let accepted = config.refresh == RefreshAnswer::Accept
        && field("grant_type") == REFRESH_GRANT
        && field("client_id") == config.client_id
        && field("client_secret") == config.client_secret
        && field("refresh_token") == inner.tokens.refresh;
    if !accepted {
        return (StatusCode::BAD_REQUEST, json!({ "error": OAUTH_REFUSAL }));
    }
    let tokens = &mut inner.tokens;
    tokens.rotations += 1;
    tokens.access = format!("{}-refreshed-{}", config.access_token, tokens.rotations);
    tokens.refresh = format!("{}-refreshed-{}", config.refresh_token, tokens.rotations);
    (
        StatusCode::OK,
        json!({
            "access_token": tokens.access,
            "token_type": "Bearer",
            "refresh_token": tokens.refresh,
            "expires_in": TOKEN_LIFETIME_SECS,
            "scope": TOKEN_SCOPES
        }),
    )
}
