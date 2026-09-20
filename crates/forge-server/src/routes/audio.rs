use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;

use crate::audio_clips::ClipOutcome;
use crate::routes::overlays::cors_header_value;
use crate::server::AppState;

const CLIP_PATH_PREFIX: &str = "/audio/v1/clip/";
const REPORT_PATH_PREFIX: &str = "/audio/v1/report/";
const CAPABILITY_PATTERN: &str = "{capability}";

const MAX_REPORT_BODY_BYTES: usize = 2 * 1024;
const MAX_REASON_CHARS: usize = 200;
const NO_STORE: &str = "no-store";

pub(crate) fn clip_path(capability: &str) -> String {
    format!("{CLIP_PATH_PREFIX}{capability}")
}

pub(crate) fn report_path(capability: &str) -> String {
    format!("{REPORT_PATH_PREFIX}{capability}")
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            &format!("{CLIP_PATH_PREFIX}{CAPABILITY_PATTERN}"),
            get(serve_clip),
        )
        .route(
            &format!("{REPORT_PATH_PREFIX}{CAPABILITY_PATTERN}"),
            post(accept_report).layer(DefaultBodyLimit::max(MAX_REPORT_BODY_BYTES)),
        )
}

async fn serve_clip(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(capability): Path<String>,
) -> Response {
    let Some(payload) = state.audio_clips.take_clip(&capability) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mut response_headers = cross_origin_headers(&state, &headers);
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(payload.media_type.as_str()),
    );
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(NO_STORE));

    (StatusCode::OK, response_headers, Body::from(payload.bytes)).into_response()
}

#[derive(Deserialize)]
#[serde(tag = "verdict", rename_all = "camelCase")]
enum VerdictBody {
    Played,
    Refused {
        #[serde(default)]
        reason: String,
    },
}

async fn accept_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(capability): Path<String>,
    Json(body): Json<VerdictBody>,
) -> Response {
    let outcome = match body {
        VerdictBody::Played => ClipOutcome::Played,
        VerdictBody::Refused { reason } => ClipOutcome::Refused {
            reason: reason.chars().take(MAX_REASON_CHARS).collect(),
        },
    };

    if !state.audio_clips.record_verdict(&capability, outcome) {
        return StatusCode::NOT_FOUND.into_response();
    }

    (
        StatusCode::NO_CONTENT,
        cross_origin_headers(&state, &headers),
    )
        .into_response()
}

fn cross_origin_headers(state: &AppState, request_headers: &HeaderMap) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        cors_header_value(state, request_headers),
    );
    headers.insert(header::VARY, HeaderValue::from_name(header::ORIGIN));
    headers
}
