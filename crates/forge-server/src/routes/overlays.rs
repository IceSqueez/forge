use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::protocol::mime_for_extension;
use crate::server::AppState;

#[derive(serde::Deserialize)]
pub struct TokenQuery {
    token: Option<String>,
}

pub async fn serve_overlay_file(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
    Query(query): Query<TokenQuery>,
    req_headers: axum::http::HeaderMap,
) -> Response {
    if state.http_overlay_require_token {
        let bearer = req_headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(str::trim)
            .map(str::to_owned);

        let token = bearer.or_else(|| query.token.clone());

        let authorized = match token {
            Some(t) => state.auth.verify(&t).await,
            None => false,
        };

        if !authorized {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }

    match resolve_and_read(&state, &path).await {
        Ok(body_bytes) => {
            let ext = std::path::Path::new(&path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
            let mime = mime_for_extension(ext).unwrap_or("application/octet-stream");
            let cors_value = cors_header_value(&state);

            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
                    (
                        axum::http::HeaderName::from_static("access-control-allow-origin"),
                        cors_value,
                    ),
                ],
                Body::from(body_bytes),
            )
                .into_response()
        }
        Err(status) => status.into_response(),
    }
}

fn cors_header_value(state: &AppState) -> HeaderValue {
    if state.overlay_cors_any_origin {
        HeaderValue::from_static("*")
    } else {
        let addr = format!("http://{}", state.bind_addr);
        HeaderValue::from_str(&addr).unwrap_or_else(|_| HeaderValue::from_static("*"))
    }
}

async fn resolve_and_read(state: &AppState, url_path: &str) -> Result<Vec<u8>, StatusCode> {
    if url_path.split('/').any(|seg| seg.starts_with('.')) {
        return Err(StatusCode::NOT_FOUND);
    }

    if url_path.contains("..") {
        return Err(StatusCode::NOT_FOUND);
    }

    let root = state.overlay_root.as_ref();
    let joined = root.join(url_path.trim_start_matches('/'));

    let canon_root = tokio::fs::canonicalize(root)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let canon_file = tokio::fs::canonicalize(&joined)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    if !canon_file.starts_with(&canon_root) {
        return Err(StatusCode::NOT_FOUND);
    }

    let meta = tokio::fs::metadata(&canon_file)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    if meta.is_dir() {
        return Err(StatusCode::NOT_FOUND);
    }

    tokio::fs::read(&canon_file)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_registry::SubActionRegistry;
    use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry, spawn_action_engine};
    use forge_storage::{
        CredentialId, CredentialsRepo, DataProvider, GlobalsRepo, StorageError, UserGlobalsRepo,
    };
    use time::OffsetDateTime;
    use tokio::net::TcpListener;

    use crate::ServerHandle;
    use crate::auth::AuthState;
    use crate::bus_adapter::BusAdapter;
    use crate::server::AppState;
    use crate::server_info::ServerInfo;
    use crate::test_helpers::test_dp;

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

    async fn make_overlay_server(
        overlay_root: std::path::PathBuf,
        http_overlay_require_token: bool,
        overlay_cors_any_origin: bool,
        creds: Arc<MemCreds>,
    ) -> (ServerHandle, SocketAddr) {
        let auth = AuthState::load(false, &*creds).await.expect("auth load");
        let creds_dyn: Arc<dyn CredentialsRepo> = creds;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let bus_adapter = BusAdapter::new(Arc::clone(&bus));
        bus_adapter.spawn();
        let dp: Arc<dyn DataProvider> = test_dp();
        let actions = dp.action_repo();
        let globals: Arc<dyn GlobalsRepo> = Arc::clone(&dp) as Arc<dyn GlobalsRepo>;
        let user_globals: Arc<dyn UserGlobalsRepo> = Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>;
        let _registry = Arc::new(ScriptRegistry::new());
        let action_engine = Arc::new(spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(SubActionRegistry::new()),
            Arc::new(forge_runtime::ActionCancelRegistry::new()),
        ));
        let bind_addr: SocketAddr = "127.0.0.1:9515".parse().expect("addr");
        let state = AppState {
            auth,
            bus,
            bus_adapter,
            actions,
            globals,
            user_globals,
            credentials: creds_dyn,
            settings: Arc::clone(&dp) as Arc<dyn forge_storage::SettingsRepo>,
            server_info: ServerInfo::new(),
            action_engine,
            overlay_root: Arc::new(overlay_root),
            http_overlay_require_token,
            overlay_cors_any_origin,
            bind_addr,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let (join, shutdown_tx) = crate::server::serve_on_with_shutdown(listener, state.clone());
        (ServerHandle::new(join, shutdown_tx, state, addr), addr)
    }

    #[tokio::test]
    async fn serve_overlay_returns_200_for_existing_file_with_correct_mime() {
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("alerts.html"), b"<h1>Alert</h1>")
            .await
            .expect("write");

        let (handle, addr) =
            make_overlay_server(dir.path().to_path_buf(), false, true, MemCreds::new()).await;

        let resp = reqwest::get(format!("http://{}/overlays/alerts.html", addr))
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 200);
        let ct = resp.headers().get("content-type").expect("content-type");
        assert!(ct.to_str().unwrap().contains("text/html"));
        let body = resp.bytes().await.expect("body");
        assert_eq!(&body[..], b"<h1>Alert</h1>");

        handle.abort();
    }

    #[tokio::test]
    async fn serve_overlay_returns_404_for_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");

        let (handle, addr) =
            make_overlay_server(dir.path().to_path_buf(), false, true, MemCreds::new()).await;

        let resp = reqwest::get(format!("http://{}/overlays/nope.html", addr))
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 404);

        handle.abort();
    }

    #[tokio::test]
    async fn serve_overlay_returns_404_for_path_traversal_attempt() {
        let dir = tempfile::tempdir().expect("tempdir");

        let (handle, addr) =
            make_overlay_server(dir.path().to_path_buf(), false, true, MemCreds::new()).await;

        let resp = reqwest::get(format!("http://{}/overlays/../../etc/passwd", addr))
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 404);

        handle.abort();
    }

    #[tokio::test]
    async fn serve_overlay_returns_404_for_hidden_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join(".secret"), b"hidden")
            .await
            .expect("write");

        let (handle, addr) =
            make_overlay_server(dir.path().to_path_buf(), false, true, MemCreds::new()).await;

        let resp = reqwest::get(format!("http://{}/overlays/.secret", addr))
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 404);

        handle.abort();
    }

    #[tokio::test]
    async fn serve_overlay_returns_cors_star_when_any_origin_true() {
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("widget.js"), b"console.log('hi')")
            .await
            .expect("write");

        let (handle, addr) =
            make_overlay_server(dir.path().to_path_buf(), false, true, MemCreds::new()).await;

        let resp = reqwest::get(format!("http://{}/overlays/widget.js", addr))
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 200);
        let cors = resp
            .headers()
            .get("access-control-allow-origin")
            .expect("cors header");
        assert_eq!(cors.to_str().unwrap(), "*");

        handle.abort();
    }

    #[tokio::test]
    async fn serve_overlay_returns_cors_bind_addr_when_any_origin_false() {
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("widget.js"), b"console.log('hi')")
            .await
            .expect("write");

        let (handle, addr) =
            make_overlay_server(dir.path().to_path_buf(), false, false, MemCreds::new()).await;

        let resp = reqwest::get(format!("http://{}/overlays/widget.js", addr))
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 200);
        let cors = resp
            .headers()
            .get("access-control-allow-origin")
            .expect("cors header");
        let cors_str = cors.to_str().unwrap();
        assert_ne!(cors_str, "*");
        assert!(cors_str.starts_with("http://127.0.0.1"));

        handle.abort();
    }

    #[tokio::test]
    async fn serve_overlay_requires_bearer_when_require_token_true() {
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("priv.html"), b"<p>private</p>")
            .await
            .expect("write");

        let creds = MemCreds::with_token("overlay-secret");
        let (handle, addr) = make_overlay_server(dir.path().to_path_buf(), true, true, creds).await;

        let resp = reqwest::get(format!("http://{}/overlays/priv.html", addr))
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 401);

        let resp = reqwest::Client::new()
            .get(format!("http://{}/overlays/priv.html", addr))
            .header("Authorization", "Bearer overlay-secret")
            .send()
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 200);

        handle.abort();
    }
}
