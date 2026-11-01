use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Clone, Copy)]
pub(crate) enum RefusalCode {
    Unauthenticated,
    OriginNotAllowed,
    HostNotAllowed,
}

impl RefusalCode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Unauthenticated => "UNAUTHENTICATED",
            Self::OriginNotAllowed => "ORIGIN_NOT_ALLOWED",
            Self::HostNotAllowed => "HOST_NOT_ALLOWED",
        }
    }
}

pub(crate) fn refusal_response(status: StatusCode, code: RefusalCode, message: &str) -> Response {
    let body = serde_json::json!({ "error": { "code": code.as_str(), "message": message } });
    (status, Json(body)).into_response()
}
