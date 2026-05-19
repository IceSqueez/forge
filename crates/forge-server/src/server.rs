use axum::{Router, routing::any, routing::get};
use tokio::net::TcpListener;

use crate::routes::{api, overlays, ws};
use crate::{ServerConfig, ServerError, ServerHandle};

pub struct Server {
    pub config: ServerConfig,
}

impl Server {
    pub async fn start(self) -> Result<ServerHandle, ServerError> {
        let addr = self.config.bind_addr;
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| ServerError::Bind {
                addr: addr.to_string(),
                reason: e.to_string(),
            })?;
        Ok(serve_on(listener))
    }
}

pub async fn start_server(config: ServerConfig) -> Result<ServerHandle, ServerError> {
    Server { config }.start().await
}

fn build_router() -> Router {
    Router::new()
        .route("/ws/v1/", get(ws::ws_handler))
        .route("/api/v1/{*path}", any(api::api_not_implemented))
        .route("/overlays/{*path}", get(overlays::overlays_not_implemented))
}

fn serve_on(listener: TcpListener) -> ServerHandle {
    let app = build_router();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .map_err(|e| ServerError::Io(std::io::Error::other(e)))
    });
    ServerHandle::new(handle)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use tokio::net::TcpListener;

    use super::serve_on;

    async fn start_server() -> (super::ServerHandle, std::net::SocketAddr) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let handle = serve_on(listener);
        (handle, addr)
    }

    #[tokio::test]
    async fn start_returns_handle_on_ephemeral_port() {
        let (handle, _) = start_server().await;
        handle.abort();
    }

    #[tokio::test]
    async fn abort_does_not_panic() {
        let (handle, _) = start_server().await;
        handle.abort();
    }

    #[tokio::test]
    async fn server_accepts_tcp_connections() {
        let (handle, addr) = start_server().await;
        tokio::net::TcpStream::connect(addr)
            .await
            .expect("tcp connect");
        handle.abort();
    }

    #[tokio::test]
    async fn api_v1_any_path_returns_501() {
        let (handle, addr) = start_server().await;
        let url = format!("http://{}/api/v1/anything", addr);
        let resp = reqwest::get(&url).await.expect("HTTP request");
        assert_eq!(resp.status().as_u16(), 501);
        let body: serde_json::Value = resp.json().await.expect("JSON body");
        assert_eq!(body["error"], "method not implemented");
        handle.abort();
    }
}
