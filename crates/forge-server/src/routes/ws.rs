use axum::extract::WebSocketUpgrade;
use axum::extract::ws::{Message, WebSocket};
use axum::response::Response;

pub async fn ws_handler(upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    let hello = serde_json::json!({
        "id": null,
        "status": "ok",
        "info": "forge-server WS v1 ready"
    });
    let _ = socket.send(Message::Text(hello.to_string().into())).await;
}
