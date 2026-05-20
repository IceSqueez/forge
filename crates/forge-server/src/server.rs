use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router, middleware};
use tokio::net::TcpListener;

use forge_runtime::{ActionEngineHandle, EventBus};
use forge_storage::{CredentialsRepo, DataProvider};

use crate::auth::AuthState;
use crate::bus_adapter::BusAdapter;
use crate::routes::{api_v1, overlays, ws};
use crate::server_info::ServerInfo;
use crate::{ServerConfig, ServerError, ServerHandle};

#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<AuthState>,
    pub bus: Arc<EventBus>,
    pub bus_adapter: Arc<BusAdapter>,
    pub dp: Arc<dyn DataProvider>,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub server_info: Arc<ServerInfo>,
    pub action_engine: Arc<ActionEngineHandle>,
    pub overlay_root: Arc<std::path::PathBuf>,
}

pub struct Server {
    pub config: ServerConfig,
}

impl Server {
    pub async fn start(self) -> Result<ServerHandle, ServerError> {
        let addr = self.config.bind_addr;
        let credentials = Arc::clone(&self.config.credentials);
        let auth = AuthState::load(
            self.config.auth_required_for_reads,
            self.config.credentials.as_ref(),
        )
        .await?;
        let bus = Arc::clone(&self.config.event_bus);
        let bus_adapter = BusAdapter::new(Arc::clone(&bus));
        bus_adapter.spawn();
        let overlay_root = Arc::new(self.config.overlay_root.clone());
        let state = AppState {
            auth,
            bus,
            bus_adapter,
            dp: self.config.data_provider,
            credentials,
            server_info: ServerInfo::new(),
            action_engine: self.config.action_engine,
            overlay_root,
        };
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| ServerError::Bind {
                addr: addr.to_string(),
                reason: e.to_string(),
            })?;
        Ok(serve_on(listener, state))
    }
}

pub async fn start_server(config: ServerConfig) -> Result<ServerHandle, ServerError> {
    Server { config }.start().await
}

async fn auth_middleware(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let is_mutating = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );

    if is_mutating || state.auth.auth_required_for_reads {
        let maybe_token = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(str::trim);

        let authorized = match maybe_token {
            Some(token) => state.auth.verify(token).await,
            None => false,
        };

        if !authorized {
            return unauthenticated_response();
        }
    }

    next.run(request).await
}

fn unauthenticated_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "error": {
                "code": "UNAUTHENTICATED",
                "message": "Bearer token required"
            }
        })),
    )
        .into_response()
}

fn build_router(state: AppState) -> Router {
    let api_routes = api_v1::router().route_layer(middleware::from_fn_with_state(
        state.clone(),
        auth_middleware,
    ));

    Router::new()
        .route("/ws/v1/", get(ws::ws_handler))
        .nest("/api/v1", api_routes)
        .route("/overlays/{*path}", get(overlays::overlays_not_implemented))
        .with_state(state)
}

#[cfg(test)]
pub(crate) fn build_router_for_test(state: AppState) -> Router {
    build_router(state)
}

fn serve_on(listener: TcpListener, state: AppState) -> ServerHandle {
    let app = build_router(state).into_make_service_with_connect_info::<SocketAddr>();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .map_err(|e| ServerError::Io(std::io::Error::other(e)))
    });
    ServerHandle::new(handle)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry, spawn_action_engine};
    use forge_storage::{CredentialId, CredentialsRepo, DataProvider, StorageError};
    use time::OffsetDateTime;
    use tokio::net::TcpListener;

    use super::{AppState, AuthState, BusAdapter, ServerHandle, serve_on};
    use crate::server_info::ServerInfo;
    use crate::test_dp::null_dp;

    struct MemCreds(Mutex<HashMap<String, String>>);

    impl MemCreds {
        fn new() -> Arc<Self> {
            Arc::new(Self(Mutex::new(HashMap::new())))
        }

        fn with_token(token: &str) -> Arc<Self> {
            let me = Self::new();
            me.0.lock()
                .expect("mutex")
                .insert("server:bearer".to_owned(), token.to_owned());
            me
        }
    }

    #[async_trait]
    impl CredentialsRepo for MemCreds {
        async fn store(
            &self,
            id: &CredentialId,
            plaintext_bundle: &str,
        ) -> Result<(), StorageError> {
            self.0
                .lock()
                .expect("mutex")
                .insert(id.as_str().to_owned(), plaintext_bundle.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.0.lock().expect("mutex").get(id.as_str()).cloned())
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.0.lock().expect("mutex").remove(id.as_str()).is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self
                .0
                .lock()
                .expect("mutex")
                .keys()
                .map(|k| CredentialId::new(k.clone()))
                .collect())
        }

        async fn last_refresh(
            &self,
            _id: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    fn make_app_state(auth: Arc<AuthState>, creds: Arc<dyn CredentialsRepo>) -> AppState {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let bus_adapter = BusAdapter::new(Arc::clone(&bus));
        bus_adapter.spawn();
        let dp: Arc<dyn DataProvider> = null_dp();
        let registry = Arc::new(ScriptRegistry::new());
        let action_engine = Arc::new(spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            registry,
            None,
        ));
        AppState {
            auth,
            bus,
            bus_adapter,
            dp,
            credentials: creds,
            server_info: ServerInfo::new(),
            action_engine,
            overlay_root: Arc::new(std::path::PathBuf::from("/tmp/forge-test-overlays")),
        }
    }

    async fn make_server(
        auth_required_for_reads: bool,
        creds: Arc<MemCreds>,
    ) -> (ServerHandle, std::net::SocketAddr) {
        let auth = AuthState::load(auth_required_for_reads, &*creds)
            .await
            .expect("auth load");
        let creds_dyn: Arc<dyn CredentialsRepo> = creds;
        let state = make_app_state(auth, creds_dyn);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let handle = serve_on(listener, state);
        (handle, addr)
    }

    async fn make_server_with_shared_auth(
        auth_required_for_reads: bool,
        creds: Arc<MemCreds>,
    ) -> (ServerHandle, std::net::SocketAddr, Arc<AuthState>) {
        let auth = AuthState::load(auth_required_for_reads, &*creds)
            .await
            .expect("auth load");
        let auth_ref = Arc::clone(&auth);
        let creds_dyn: Arc<dyn CredentialsRepo> = creds;
        let state = make_app_state(auth, creds_dyn);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let handle = serve_on(listener, state);
        (handle, addr, auth_ref)
    }

    #[tokio::test]
    async fn start_returns_handle_on_ephemeral_port() {
        let (handle, _) = make_server(false, MemCreds::new()).await;
        handle.abort();
    }

    #[tokio::test]
    async fn abort_does_not_panic() {
        let (handle, _) = make_server(false, MemCreds::new()).await;
        handle.abort();
    }

    #[tokio::test]
    async fn server_accepts_tcp_connections() {
        let (handle, addr) = make_server(false, MemCreds::new()).await;
        tokio::net::TcpStream::connect(addr)
            .await
            .expect("tcp connect");
        handle.abort();
    }

    #[tokio::test]
    async fn get_info_without_auth_returns_200_when_reads_not_required() {
        let (handle, addr) = make_server(false, MemCreds::new()).await;
        let url = format!("http://{}/api/v1/info", addr);
        let resp = reqwest::get(&url).await.expect("HTTP request");
        assert_eq!(resp.status().as_u16(), 200);
        handle.abort();
    }

    #[tokio::test]
    async fn post_without_auth_returns_401() {
        let creds = MemCreds::with_token("secret-token");
        let (handle, addr) = make_server(false, creds).await;
        let url = format!("http://{}/api/v1/actions/test-id:do", addr);
        let resp = reqwest::Client::new()
            .post(&url)
            .send()
            .await
            .expect("HTTP request");
        assert_eq!(resp.status().as_u16(), 401);
        let body: serde_json::Value = resp.json().await.expect("JSON body");
        assert_eq!(body["error"]["code"], "UNAUTHENTICATED");
        handle.abort();
    }

    #[tokio::test]
    async fn post_with_valid_bearer_passes_auth() {
        let creds = MemCreds::with_token("secret-token");
        let (handle, addr) = make_server(false, creds).await;
        let url = format!("http://{}/api/v1/actions/test-id:do", addr);
        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", "Bearer secret-token")
            .send()
            .await
            .expect("HTTP request");
        assert!(
            resp.status().as_u16() != 401,
            "expected auth to pass, got 401"
        );
        handle.abort();
    }

    #[tokio::test]
    async fn post_with_wrong_bearer_returns_401() {
        let creds = MemCreds::with_token("secret-token");
        let (handle, addr) = make_server(false, creds).await;
        let url = format!("http://{}/api/v1/actions/test-id:do", addr);
        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", "Bearer wrong-token")
            .send()
            .await
            .expect("HTTP request");
        assert_eq!(resp.status().as_u16(), 401);
        handle.abort();
    }

    #[tokio::test]
    async fn get_without_auth_returns_401_when_reads_required() {
        let creds = MemCreds::with_token("secret-token");
        let (handle, addr) = make_server(true, creds).await;
        let url = format!("http://{}/api/v1/info", addr);
        let resp = reqwest::get(&url).await.expect("HTTP request");
        assert_eq!(resp.status().as_u16(), 401);
        handle.abort();
    }

    #[tokio::test]
    async fn get_with_valid_auth_passes_when_reads_required() {
        let creds = MemCreds::with_token("secret-token");
        let (handle, addr) = make_server(true, creds).await;
        let url = format!("http://{}/api/v1/info", addr);
        let resp = reqwest::Client::new()
            .get(&url)
            .header("Authorization", "Bearer secret-token")
            .send()
            .await
            .expect("HTTP request");
        assert_eq!(resp.status().as_u16(), 200);
        handle.abort();
    }

    #[tokio::test]
    async fn token_regenerate_rejects_old_and_accepts_new() {
        let creds = MemCreds::with_token("old-token");
        let (handle, addr, auth) = make_server_with_shared_auth(false, Arc::clone(&creds)).await;
        let url = format!("http://{}/api/v1/actions/test-id:do", addr);

        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", "Bearer old-token")
            .send()
            .await
            .expect("request");
        assert!(
            resp.status().as_u16() != 401,
            "old token should be accepted before regenerate"
        );

        let new_token = auth.regenerate(&*creds).await.expect("regenerate");

        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", "Bearer old-token")
            .send()
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 401);

        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {}", new_token))
            .send()
            .await
            .expect("request");
        assert!(
            resp.status().as_u16() != 401,
            "new token should be accepted after regenerate"
        );

        handle.abort();
    }
}
