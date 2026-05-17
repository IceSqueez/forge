use axum::{Router, routing::get};
use tokio::net::TcpListener;

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

fn serve_on(listener: TcpListener) -> ServerHandle {
    let app: Router = Router::new().route("/", get(|| async { "forge server" }));
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
    async fn get_root_returns_200() {
        let (handle, addr) = start_server().await;
        let url = format!("http://{}/", addr);
        let resp = reqwest::get(&url).await.expect("HTTP request");
        assert_eq!(resp.status().as_u16(), 200);
        handle.abort();
    }
}
