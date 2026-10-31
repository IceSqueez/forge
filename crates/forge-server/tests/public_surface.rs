#![expect(clippy::unwrap_used, reason = "test fixtures")]

use std::net::SocketAddr;
use std::time::Duration;

use forge_server::test_support::ServerDeps;
use forge_server::{OVERLAY_ROUTE, ServerHandle, overlay_file_url, start_server};
use futures_util::{SinkExt, StreamExt};
use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

const HANG_GUARD: Duration = Duration::from_secs(30);
const PAGE_NAME: &str = "page.html";
const PAGE_BODY: &str = "<p>live</p>";
const REQUEST_ID: &str = "probe-1";

struct Running {
    handle: ServerHandle,
    addr: SocketAddr,
    _overlay_root: tempfile::TempDir,
}

async fn running() -> Running {
    let overlay_root = tempfile::tempdir().unwrap();
    std::fs::write(overlay_root.path().join(PAGE_NAME), PAGE_BODY).unwrap();
    let handle = start_server(ServerDeps::new(overlay_root.path()).build())
        .await
        .unwrap();
    let addr = handle.bind_addr().await;
    Running {
        handle,
        addr,
        _overlay_root: overlay_root,
    }
}

impl Running {
    async fn overlay_url(&self, file_name: &str) -> String {
        let token = self.handle.overlay_token().await;
        overlay_file_url(&format!("http://{}", self.addr), token.expose(), file_name)
    }

    async fn ws_answer(&self, request: Value) -> Value {
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://{}/ws/v1/", self.addr))
                .await
                .unwrap();
        socket
            .send(Message::text(request.to_string()))
            .await
            .unwrap();
        tokio::time::timeout(HANG_GUARD, async {
            loop {
                let frame = socket.next().await.unwrap().unwrap();
                if let Message::Text(text) = frame {
                    let answer: Value = serde_json::from_str(text.as_str()).unwrap();
                    if answer["id"] == REQUEST_ID {
                        return answer;
                    }
                }
            }
        })
        .await
        .unwrap()
    }
}

#[tokio::test]
async fn an_overlay_page_is_served_under_the_overlay_token() {
    let server = running().await;

    let response = reqwest::get(server.overlay_url(PAGE_NAME).await)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.text().await.unwrap(), PAGE_BODY);
    server.handle.stop().await.unwrap();
}

#[tokio::test]
async fn an_overlay_page_is_refused_without_the_token() {
    let server = running().await;

    let response = reqwest::get(format!(
        "http://{}/{OVERLAY_ROUTE}/not-the-token/{PAGE_NAME}",
        server.addr
    ))
    .await
    .unwrap();

    assert_ne!(response.status(), StatusCode::OK);
    assert_ne!(response.text().await.unwrap(), PAGE_BODY);
    server.handle.stop().await.unwrap();
}

#[tokio::test]
async fn a_websocket_client_is_answered_under_its_request_id() {
    let server = running().await;

    let answer = server
        .ws_answer(json!({ "id": REQUEST_ID, "request": "getInfo" }))
        .await;

    assert_eq!(answer["status"], "ok");
    server.handle.stop().await.unwrap();
}

#[tokio::test]
async fn a_websocket_mutation_without_authentication_is_refused() {
    let server = running().await;

    let answer = server
        .ws_answer(json!({
            "id": REQUEST_ID,
            "request": "setGlobal",
            "name": "score",
            "value": 1,
        }))
        .await;

    assert_eq!(answer["status"], "error");
    assert_eq!(answer["error"]["code"], "UNAUTHENTICATED");
    server.handle.stop().await.unwrap();
}
