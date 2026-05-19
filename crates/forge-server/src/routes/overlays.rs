use axum::http::StatusCode;

pub async fn overlays_not_implemented() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
