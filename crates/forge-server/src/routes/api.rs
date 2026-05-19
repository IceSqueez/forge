use axum::Json;
use axum::http::StatusCode;

pub async fn api_not_implemented() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({"error": "method not implemented"})),
    )
}
