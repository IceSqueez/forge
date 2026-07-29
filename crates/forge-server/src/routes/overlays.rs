use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use forge_storage::OverlayId;

use crate::protocol::mime_for_extension;
use crate::server::AppState;

const OVERLAY_ENTRY_DOCUMENT: &str = "index.html";

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

    if !overlay_serving_enabled(state, url_path).await {
        return Err(StatusCode::NOT_FOUND);
    }

    let root = state.overlay_root.as_ref();
    let trimmed = url_path.trim_start_matches('/');
    let requested = std::path::Path::new(trimmed);

    let canon_target = crate::sandbox::confine(root, requested)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let meta = tokio::fs::metadata(&canon_target)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let canon_file = if meta.is_dir() {
        let entry_relative = std::path::Path::new(trimmed).join(OVERLAY_ENTRY_DOCUMENT);
        let canon_entry = crate::sandbox::confine(root, &entry_relative)
            .await
            .ok_or(StatusCode::NOT_FOUND)?;
        let entry_meta = tokio::fs::metadata(&canon_entry)
            .await
            .map_err(|_| StatusCode::NOT_FOUND)?;
        if !entry_meta.is_file() {
            return Err(StatusCode::NOT_FOUND);
        }
        canon_entry
    } else {
        canon_target
    };

    tokio::fs::read(&canon_file)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)
}

async fn overlay_serving_enabled(state: &AppState, url_path: &str) -> bool {
    let Some(identity) = url_path
        .trim_start_matches('/')
        .split('/')
        .next()
        .filter(|seg| !seg.is_empty())
    else {
        return true;
    };

    match state.overlays.get(&OverlayId::new(identity)).await {
        Ok(Some(definition)) => definition.enabled,
        Ok(None) | Err(_) => true,
    }
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
        let overlays = dp.overlay_repo();
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
            overlays,
            credentials: creds_dyn,
            settings: Arc::clone(&dp) as Arc<dyn forge_storage::SettingsRepo>,
            server_info: ServerInfo::new(),
            action_engine,
            overlay_root: Arc::new(overlay_root),
            http_overlay_require_token,
            overlay_cors_any_origin,
            bind_addr,
            allowed_origins: Arc::new(crate::origin::build_allowed_origins(bind_addr, &[])),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let (run_state_tx, _run_state_rx) = tokio::sync::watch::channel(true);
        let (join, shutdown_tx) =
            crate::server::serve_on_with_shutdown(listener, state.clone(), run_state_tx.clone());
        (
            ServerHandle::new(join, shutdown_tx, state, addr, run_state_tx),
            addr,
        )
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

    /// Sends the request target verbatim; `reqwest` resolves dot segments while parsing a URL,
    /// so an encoded-traversal probe never reaches the handler through the typed client.
    async fn raw_get(addr: SocketAddr, target: &str, extra_headers: &str) -> (u16, String) {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let request = format!(
            "GET {target} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n{extra_headers}\r\n"
        );
        stream
            .write_all(request.as_bytes())
            .await
            .expect("write request");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.expect("read response");
        let text = String::from_utf8_lossy(&raw).into_owned();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .expect("status line");
        (status, text)
    }

    fn qa_tempdir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("forge-qa-overlay")
            .tempdir()
            .expect("tempdir")
    }

    const ESCAPE_MARKER: &str = "ESCAPED-OVERLAY-ROOT";

    /// Returns (root, outside-file-name) with the decoy written as a sibling of the overlay root.
    async fn root_with_outside_decoy(base: &std::path::Path) -> (std::path::PathBuf, &'static str) {
        let root = base.join("root");
        tokio::fs::create_dir(&root).await.expect("create root");
        tokio::fs::write(base.join("secret.html"), ESCAPE_MARKER.as_bytes())
            .await
            .expect("write decoy");
        (root, "secret.html")
    }

    #[tokio::test]
    async fn serve_overlay_rejects_encoded_traversal_that_survives_url_normalization() {
        let dir = qa_tempdir();
        let (root, _) = root_with_outside_decoy(dir.path()).await;

        let (handle, addr) = make_overlay_server(root, false, true, MemCreds::new()).await;

        for target in [
            "/overlays/%2e%2e%2fsecret.html",
            "/overlays/..%2fsecret.html",
            "/overlays/%2E%2E%2Fsecret.html",
            "/overlays/sub%2f%2e%2e%2f%2e%2e%2fsecret.html",
            "/overlays/.%2e%2fsecret.html",
        ] {
            let (status, body) = raw_get(addr, target, "").await;
            assert_eq!(status, 404, "expected 404 for {target}");
            assert!(
                !body.contains(ESCAPE_MARKER),
                "escaped the overlay root via {target}"
            );
        }

        handle.abort();
    }

    #[tokio::test]
    async fn serve_overlay_rejects_absolute_request_path_instead_of_rerooting() {
        let dir = qa_tempdir();
        let (root, _) = root_with_outside_decoy(dir.path()).await;
        let outside = dir.path().join("secret.html");
        let absolute = outside.to_str().expect("utf8 path");
        let encoded = absolute.replace('/', "%2F");

        let (handle, addr) = make_overlay_server(root, false, true, MemCreds::new()).await;

        for target in [
            format!("/overlays/{encoded}"),
            format!("/overlays/{absolute}"),
        ] {
            let (status, body) = raw_get(addr, &target, "").await;
            assert_eq!(status, 404, "expected 404 for {target}");
            assert!(
                !body.contains(ESCAPE_MARKER),
                "absolute path re-rooted the join via {target}"
            );
        }

        handle.abort();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn serve_overlay_rejects_symlink_pointing_outside_the_root() {
        let dir = qa_tempdir();
        let (root, decoy) = root_with_outside_decoy(dir.path()).await;
        std::os::unix::fs::symlink(dir.path().join(decoy), root.join("link.html"))
            .expect("symlink");

        let (handle, addr) = make_overlay_server(root, false, true, MemCreds::new()).await;

        let (status, body) = raw_get(addr, "/overlays/link.html", "").await;
        assert_eq!(status, 404);
        assert!(!body.contains(ESCAPE_MARKER));

        handle.abort();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn serve_overlay_follows_symlink_that_resolves_inside_the_root() {
        let dir = qa_tempdir();
        let root = dir.path().join("root");
        tokio::fs::create_dir(&root).await.expect("create root");
        tokio::fs::write(root.join("real.html"), b"<p>inside</p>")
            .await
            .expect("write");
        std::os::unix::fs::symlink("real.html", root.join("alias.html")).expect("symlink");

        let (handle, addr) = make_overlay_server(root, false, true, MemCreds::new()).await;

        let (status, body) = raw_get(addr, "/overlays/alias.html", "").await;
        assert_eq!(status, 200);
        assert!(body.contains("<p>inside</p>"));

        handle.abort();
    }

    #[tokio::test]
    async fn serve_overlay_rejection_never_echoes_a_presented_credential() {
        let dir = qa_tempdir();
        tokio::fs::write(dir.path().join("priv.html"), b"<p>private</p>")
            .await
            .expect("write");

        let creds = MemCreds::with_token("overlay-secret-installation-bearer");
        let (handle, addr) = make_overlay_server(dir.path().to_path_buf(), true, true, creds).await;

        let (status, body) = raw_get(
            addr,
            "/overlays/priv.html?token=WRONG-QUERY-CREDENTIAL",
            "Authorization: Bearer WRONG-HEADER-CREDENTIAL\r\n",
        )
        .await;

        assert_eq!(status, 401);
        for secret in [
            "WRONG-QUERY-CREDENTIAL",
            "WRONG-HEADER-CREDENTIAL",
            "overlay-secret-installation-bearer",
        ] {
            assert!(!body.contains(secret), "rejection echoed {secret}");
        }

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
