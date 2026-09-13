use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};
use serde_json::{Value, json};

use super::chat::{Viewer, user_json};
use super::config::FakeTwitchConfig;
use super::ids;
use super::ledger::{CredentialCheck, RecordedRequest};
use super::state::{Inner, Shared, SubscriptionOutcome, SubscriptionRequest};

#[derive(Debug, Clone, Copy)]
enum Route {
    CreateSubscription,
    Users,
    Streams,
    Polls,
    Predictions,
    SendChatMessage,
}

impl Route {
    fn of(method: &Method, path: &str) -> Option<Self> {
        let route = match (method.as_str(), path) {
            ("POST", "/helix/eventsub/subscriptions") => Self::CreateSubscription,
            ("GET", "/helix/users") => Self::Users,
            ("GET", "/helix/streams") => Self::Streams,
            ("GET", "/helix/polls") => Self::Polls,
            ("GET", "/helix/predictions") => Self::Predictions,
            ("POST", "/helix/chat/messages") => Self::SendChatMessage,
            _ => return None,
        };
        Some(route)
    }
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
    let body = decode_body(&body);
    let credentials = check_credentials(&headers, shared.config());
    let route = Route::of(&method, uri.path());

    let (status, response) = shared.mutate(|inner| {
        let answer = match route {
            None => error_body(StatusCode::NOT_FOUND, ""),
            Some(_) if credentials != CredentialCheck::Accepted => {
                error_body(StatusCode::UNAUTHORIZED, refusal_message(credentials))
            }
            Some(route) => serve(inner, shared.config(), route, &query, body.as_ref()),
        };
        inner.record_request(RecordedRequest {
            method: method.as_str().to_owned(),
            path: uri.path().to_owned(),
            query: query.clone(),
            body: body.clone(),
            credentials,
            status: answer.0.as_u16(),
            response: answer.1.clone(),
            modeled: route.is_some(),
        });
        answer
    });
    (status, Json(response)).into_response()
}

fn decode_body(bytes: &Bytes) -> Option<Value> {
    if bytes.is_empty() {
        return None;
    }
    Some(
        serde_json::from_slice(bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned())),
    )
}

fn check_credentials(headers: &HeaderMap, config: &FakeTwitchConfig) -> CredentialCheck {
    let bearer = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    match bearer {
        None => return CredentialCheck::MissingBearer,
        Some(token) if token != config.access_token => return CredentialCheck::WrongBearer,
        Some(_) => {}
    }
    match headers
        .get("client-id")
        .and_then(|value| value.to_str().ok())
    {
        None => CredentialCheck::MissingClientId,
        Some(client_id) if client_id != config.client_id => CredentialCheck::WrongClientId,
        Some(_) => CredentialCheck::Accepted,
    }
}

fn refusal_message(check: CredentialCheck) -> &'static str {
    match check {
        CredentialCheck::MissingBearer => "OAuth token is missing",
        CredentialCheck::WrongBearer => "Invalid OAuth token",
        CredentialCheck::MissingClientId => "Client ID is missing",
        CredentialCheck::WrongClientId => "Client ID and OAuth token do not match",
        CredentialCheck::Accepted => "",
    }
}

fn error_body(status: StatusCode, message: &str) -> (StatusCode, Value) {
    let body = json!({
        "error": status.canonical_reason().unwrap_or_default(),
        "status": status.as_u16(),
        "message": message,
    });
    (status, body)
}

fn serve(
    inner: &mut Inner,
    config: &FakeTwitchConfig,
    route: Route,
    query: &[(String, String)],
    body: Option<&Value>,
) -> (StatusCode, Value) {
    match route {
        Route::CreateSubscription => create_subscription(inner, body),
        Route::Users => (
            StatusCode::OK,
            json!({ "data": users(inner, config, query) }),
        ),
        Route::Streams | Route::Polls | Route::Predictions => {
            (StatusCode::OK, json!({ "data": [], "pagination": {} }))
        }
        Route::SendChatMessage => send_chat_message(body),
    }
}

fn create_subscription(inner: &mut Inner, body: Option<&Value>) -> (StatusCode, Value) {
    let request = match parse_subscription_request(body) {
        Ok(request) => request,
        Err(reason) => return error_body(StatusCode::BAD_REQUEST, reason),
    };
    match inner.create_subscription(request) {
        SubscriptionOutcome::Created(subscription) => {
            let data = json!({
                "id": subscription.id,
                "status": "enabled",
                "type": subscription.subscription_type,
                "version": subscription.version,
                "condition": subscription.condition,
                "created_at": subscription.created_at,
                "transport": {
                    "method": "websocket",
                    "session_id": subscription.session_id,
                    "connected_at": subscription.created_at,
                },
                "cost": 0,
            });
            let body = json!({ "data": [data], "total": 1, "total_cost": 0, "max_total_cost": 10 });
            (StatusCode::ACCEPTED, body)
        }
        SubscriptionOutcome::UnknownSession => error_body(
            StatusCode::BAD_REQUEST,
            "websocket transport session does not exist or has already disconnected",
        ),
        SubscriptionOutcome::Duplicate => {
            error_body(StatusCode::CONFLICT, "subscription already exists")
        }
    }
}

fn parse_subscription_request(body: Option<&Value>) -> Result<SubscriptionRequest, &'static str> {
    let body = body.ok_or("request body is missing")?;
    let text = |value: &Value| value.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    let subscription_type = text(&body["type"]).ok_or("type is missing")?;
    let version = text(&body["version"]).ok_or("version is missing")?;
    let condition = body
        .get("condition")
        .filter(|condition| condition.is_object())
        .cloned()
        .ok_or("condition must be an object")?;
    if body["transport"]["method"].as_str() != Some("websocket") {
        return Err("transport method must be websocket");
    }
    let session_id = text(&body["transport"]["session_id"]).ok_or("session_id is missing")?;
    Ok(SubscriptionRequest {
        subscription_type,
        version,
        condition,
        session_id,
    })
}

fn users(inner: &Inner, config: &FakeTwitchConfig, query: &[(String, String)]) -> Vec<Value> {
    let values = |key: &str| {
        query
            .iter()
            .filter(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>()
    };
    let (ids, logins) = (values("id"), values("login"));
    let broadcaster = user_json(
        &config.broadcaster_user_id,
        &config.broadcaster_login,
        &config.broadcaster_login,
    );
    if ids.is_empty() && logins.is_empty() {
        return vec![broadcaster];
    }
    std::iter::once(broadcaster)
        .chain(inner.viewers.iter().map(Viewer::user_json))
        .filter(|user| {
            let id = user["id"].as_str().unwrap_or_default();
            let login = user["login"].as_str().unwrap_or_default();
            ids.contains(&id)
                || logins
                    .iter()
                    .any(|wanted| wanted.eq_ignore_ascii_case(login))
        })
        .collect()
}

fn send_chat_message(body: Option<&Value>) -> (StatusCode, Value) {
    let complete = body.is_some_and(|body| {
        ["broadcaster_id", "sender_id", "message"]
            .iter()
            .all(|field| body[*field].is_string())
    });
    if !complete {
        return error_body(
            StatusCode::BAD_REQUEST,
            "broadcaster_id, sender_id and message are required",
        );
    }
    let data = json!({ "message_id": ids::uuid_like(), "is_sent": true, "drop_reason": null });
    (StatusCode::OK, json!({ "data": [data] }))
}
