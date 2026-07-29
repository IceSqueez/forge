use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use forge_storage::OverlayId;

use crate::protocol::mime_for_extension;
use crate::server::AppState;

const OVERLAY_ENTRY_DOCUMENT: &str = "index.html";

pub async fn serve_overlay_file(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    match resolve_and_read(&state, &path).await {
        Ok((body_bytes, resolved)) => {
            let ext = resolved
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

async fn resolve_and_read(
    state: &AppState,
    url_path: &str,
) -> Result<(Vec<u8>, std::path::PathBuf), StatusCode> {
    if url_path.split('/').any(|seg| seg.starts_with('.')) {
        return Err(StatusCode::NOT_FOUND);
    }

    if url_path.contains("..") {
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

    if !overlay_serving_enabled(state, root, &canon_file).await {
        return Err(StatusCode::NOT_FOUND);
    }

    let body = tokio::fs::read(&canon_file)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((body, canon_file))
}

async fn overlay_serving_enabled(
    state: &AppState,
    root: &std::path::Path,
    canon_file: &std::path::Path,
) -> bool {
    let Ok(canon_root) = tokio::fs::canonicalize(root).await else {
        return true;
    };
    let Some(identity) = canon_file
        .strip_prefix(&canon_root)
        .ok()
        .and_then(|relative| relative.components().next())
        .and_then(|first| first.as_os_str().to_str())
    else {
        return true;
    };

    match state.overlays.get(&OverlayId::new(identity)).await {
        Ok(Some(definition)) => definition.enabled,
        Ok(None) => true,
        Err(error) => {
            tracing::warn!(%identity, %error, "overlay enabled lookup failed; serving anyway");
            true
        }
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
        CredentialId, CredentialsRepo, DataProvider, GlobalsRepo, MockOverlayRepo, OverlayConfig,
        OverlayCredential, OverlayDefinition, OverlayId, OverlayRepo, StorageError,
        UserGlobalsRepo,
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
        overlay_cors_any_origin: bool,
        creds: Arc<MemCreds>,
    ) -> (ServerHandle, SocketAddr) {
        make_overlay_server_with_overlays(overlay_root, overlay_cors_any_origin, creds, None).await
    }

    async fn make_overlay_server_with_overlays(
        overlay_root: std::path::PathBuf,
        overlay_cors_any_origin: bool,
        creds: Arc<MemCreds>,
        overlay_repo: Option<Arc<dyn OverlayRepo>>,
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
        let overlays = overlay_repo.unwrap_or_else(|| dp.overlay_repo());
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
            make_overlay_server(dir.path().to_path_buf(), true, MemCreds::new()).await;

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
            make_overlay_server(dir.path().to_path_buf(), true, MemCreds::new()).await;

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

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

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

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

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

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

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

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        let (status, body) = raw_get(addr, "/overlays/alias.html", "").await;
        assert_eq!(status, 200);
        assert!(body.contains("<p>inside</p>"));

        handle.abort();
    }

    #[tokio::test]
    async fn serve_overlay_returns_404_for_hidden_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join(".secret"), b"hidden")
            .await
            .expect("write");

        let (handle, addr) =
            make_overlay_server(dir.path().to_path_buf(), true, MemCreds::new()).await;

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
            make_overlay_server(dir.path().to_path_buf(), true, MemCreds::new()).await;

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
            make_overlay_server(dir.path().to_path_buf(), false, MemCreds::new()).await;

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

    async fn make_root(base: &std::path::Path) -> std::path::PathBuf {
        let root = base.join("root");
        tokio::fs::create_dir(&root).await.expect("create root");
        root
    }

    async fn write_at(path: std::path::PathBuf, body: &str) {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .expect("create parent");
        }
        tokio::fs::write(path, body.as_bytes())
            .await
            .expect("write");
    }

    async fn get_status_and_body(addr: SocketAddr, target: &str) -> (u16, String) {
        let resp = reqwest::get(format!("http://{addr}{target}"))
            .await
            .expect("request");
        let status = resp.status().as_u16();
        let body = resp.text().await.expect("body");
        (status, body)
    }

    #[tokio::test]
    async fn serve_overlay_resolves_a_directory_to_its_index_document_typed_as_html() {
        let dir = qa_tempdir();
        let root = make_root(dir.path()).await;
        write_at(root.join("alerts").join("index.html"), "<h1>Entry</h1>").await;

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        for target in ["/overlays/alerts/", "/overlays/alerts"] {
            let resp = reqwest::get(format!("http://{addr}{target}"))
                .await
                .expect("request");
            assert_eq!(resp.status().as_u16(), 200, "expected 200 for {target}");
            let content_type = resp
                .headers()
                .get("content-type")
                .expect("content-type")
                .to_str()
                .expect("ascii content-type")
                .to_owned();
            assert!(
                content_type.contains("text/html"),
                "index document typed as {content_type} for {target}"
            );
            let body = resp.text().await.expect("body");
            assert_eq!(body, "<h1>Entry</h1>", "wrong body for {target}");
        }

        handle.abort();
    }

    #[tokio::test]
    async fn serve_overlay_returns_404_for_a_directory_without_an_index_document() {
        let dir = qa_tempdir();
        let root = make_root(dir.path()).await;
        tokio::fs::create_dir(root.join("bare"))
            .await
            .expect("create bare");
        tokio::fs::create_dir_all(root.join("shadowed").join("index.html"))
            .await
            .expect("create shadowed");

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        for target in [
            "/overlays/bare/",
            "/overlays/bare",
            "/overlays/shadowed/",
            "/overlays/shadowed",
        ] {
            let (status, _) = get_status_and_body(addr, target).await;
            assert_eq!(status, 404, "expected 404 for {target}");
        }

        handle.abort();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn serve_overlay_rejects_a_directory_index_symlinked_outside_the_root() {
        let dir = qa_tempdir();
        let (root, decoy) = root_with_outside_decoy(dir.path()).await;
        tokio::fs::create_dir(root.join("alerts"))
            .await
            .expect("create overlay dir");
        std::os::unix::fs::symlink(
            dir.path().join(decoy),
            root.join("alerts").join("index.html"),
        )
        .expect("symlink");

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        for target in ["/overlays/alerts/", "/overlays/alerts"] {
            let (status, body) = raw_get(addr, target, "").await;
            assert_eq!(status, 404, "expected 404 for {target}");
            assert!(
                !body.contains(ESCAPE_MARKER),
                "directory index escaped the overlay root via {target}"
            );
        }

        handle.abort();
    }

    #[tokio::test]
    async fn serve_overlay_types_the_response_from_the_resolved_file_extension() {
        let dir = qa_tempdir();
        let root = make_root(dir.path()).await;
        write_at(root.join("config.json"), "{}").await;
        write_at(root.join("face.woff2"), "font-bytes").await;
        write_at(root.join("blob.unknownext"), "opaque").await;

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        for (target, expected) in [
            ("/overlays/config.json", "application/json"),
            ("/overlays/face.woff2", "font/woff2"),
            ("/overlays/blob.unknownext", "application/octet-stream"),
        ] {
            let resp = reqwest::get(format!("http://{addr}{target}"))
                .await
                .expect("request");
            assert_eq!(resp.status().as_u16(), 200, "expected 200 for {target}");
            let content_type = resp
                .headers()
                .get("content-type")
                .expect("content-type")
                .to_str()
                .expect("ascii content-type")
                .to_owned();
            assert!(
                content_type.contains(expected),
                "{target} typed as {content_type}, expected {expected}"
            );
        }

        handle.abort();
    }

    fn overlay_definition(identity: &str, enabled: bool) -> OverlayDefinition {
        OverlayDefinition {
            id: OverlayId::new(identity),
            display_name: identity.to_owned(),
            kind_id: "forge.chat".to_owned(),
            enabled,
            position: 0,
            config: OverlayConfig::new(),
            config_schema_version: 1,
            generator_version: 1,
            source_overrides: Vec::new(),
            credential: OverlayCredential::new("overlay-read-credential"),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn gated_overlay_repo() -> Arc<dyn OverlayRepo> {
        let mut repo = MockOverlayRepo::new();
        repo.expect_get().returning(|id| match id.as_str() {
            "off" => Ok(Some(overlay_definition("off", false))),
            "on" => Ok(Some(overlay_definition("on", true))),
            "boom" => Err(StorageError::Connection {
                reason: "overlay store offline".to_owned(),
            }),
            _ => Ok(None),
        });
        Arc::new(repo)
    }

    async fn gated_overlay_root(base: &std::path::Path) -> std::path::PathBuf {
        let root = make_root(base).await;
        for identity in ["off", "on", "boom", "stranger"] {
            write_at(
                root.join(identity).join("index.html"),
                &format!("<p>{identity}</p>"),
            )
            .await;
        }
        write_at(root.join("off").join("asset.js"), "console.log('off')").await;
        write_at(
            root.join("forge-shared").join("runtime-v1.js"),
            "export const runtime = 1;",
        )
        .await;
        root
    }

    #[tokio::test]
    async fn serve_overlay_returns_404_for_every_path_under_a_disabled_overlay() {
        let dir = qa_tempdir();
        let root = gated_overlay_root(dir.path()).await;

        let (handle, addr) = make_overlay_server_with_overlays(
            root,
            true,
            MemCreds::new(),
            Some(gated_overlay_repo()),
        )
        .await;

        for target in [
            "/overlays/off/",
            "/overlays/off",
            "/overlays/off/index.html",
            "/overlays/off/asset.js",
        ] {
            let (status, body) = get_status_and_body(addr, target).await;
            assert_eq!(status, 404, "expected 404 for {target}");
            assert!(
                !body.contains("<p>off</p>"),
                "disabled overlay served its body via {target}"
            );
        }

        handle.abort();
    }

    #[tokio::test]
    async fn serve_overlay_serves_unless_the_store_reports_the_identity_disabled() {
        let dir = qa_tempdir();
        let root = gated_overlay_root(dir.path()).await;

        let (handle, addr) = make_overlay_server_with_overlays(
            root,
            true,
            MemCreds::new(),
            Some(gated_overlay_repo()),
        )
        .await;

        for (target, expected_body) in [
            ("/overlays/on/index.html", "<p>on</p>"),
            ("/overlays/stranger/index.html", "<p>stranger</p>"),
            ("/overlays/boom/index.html", "<p>boom</p>"),
            (
                "/overlays/forge-shared/runtime-v1.js",
                "export const runtime = 1;",
            ),
        ] {
            let (status, body) = get_status_and_body(addr, target).await;
            assert_eq!(status, 200, "expected 200 for {target}");
            assert_eq!(body, expected_body, "wrong body for {target}");
        }

        handle.abort();
    }
}
