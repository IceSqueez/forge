use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use forge_storage::OverlayId;
use std::path::{Path, PathBuf};
use tokio_util::io::ReaderStream;

use crate::origin::accepts_origin;
use crate::protocol::mime_for_extension;
use crate::server::AppState;

const OVERLAY_ENTRY_DOCUMENT: &str = "index.html";
pub(crate) const FAVICON_PATH: &str = "/favicon.ico";
pub(crate) const NOSNIFF: &str = "nosniff";
const NO_REFERRER: &str = "no-referrer";
const ANY_ORIGIN: &str = "*";
const SELF_SCHEME: &str = "http://";

const CACHE_CONTROL_PUBLIC: &str = "public";
const CACHE_CONTROL_MAX_AGE_PARAM: &str = "max-age";
const CACHE_CONTROL_IMMUTABLE: &str = "immutable";
const SECONDS_PER_MINUTE: u64 = 60;
const MINUTES_PER_HOUR: u64 = 60;
const HOURS_PER_DAY: u64 = 24;
const DAYS_PER_YEAR: u64 = 365;
const GENERATED_MEDIA_CACHE_MAX_AGE_SECONDS: u64 =
    SECONDS_PER_MINUTE * MINUTES_PER_HOUR * HOURS_PER_DAY * DAYS_PER_YEAR;

pub async fn serve_overlay_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(requested): axum::extract::Path<String>,
) -> Response {
    let Some(path) = strip_overlay_token(&state, &requested) else {
        return refused_response();
    };
    match resolve_and_open(&state, path).await {
        Ok(served) => {
            let resolved = served.path;
            let ext = resolved
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
            let mime = mime_for_extension(ext).unwrap_or("application/octet-stream");
            let cors_value = if is_overlay_config(&resolved) {
                scoped_cors_header_value(&state, &headers)
            } else {
                cors_header_value(&state, &headers)
            };

            let mut response_headers = HeaderMap::new();
            response_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
            response_headers.insert(header::CONTENT_LENGTH, HeaderValue::from(served.len));
            response_headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, cors_value);
            response_headers.insert(header::VARY, HeaderValue::from_name(header::ORIGIN));
            response_headers.insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static(NOSNIFF),
            );
            response_headers.insert(
                header::REFERRER_POLICY,
                HeaderValue::from_static(NO_REFERRER),
            );
            if is_generated_media_path(path) {
                response_headers.insert(header::CACHE_CONTROL, immutable_cache_control());
            }

            let body = Body::from_stream(ReaderStream::new(served.file));
            (StatusCode::OK, response_headers, body).into_response()
        }
        Err(status) => (
            status,
            [(
                header::REFERRER_POLICY,
                HeaderValue::from_static(NO_REFERRER),
            )],
        )
            .into_response(),
    }
}

fn refused_response() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(
            header::REFERRER_POLICY,
            HeaderValue::from_static(NO_REFERRER),
        )],
    )
        .into_response()
}

fn strip_overlay_token<'a>(state: &AppState, requested: &'a str) -> Option<&'a str> {
    let token_gate = state.auth.overlay_token();
    let (first, remainder) = requested.split_once('/').unwrap_or((requested, ""));
    if token_gate.verify(first) {
        return (!remainder.is_empty()).then_some(remainder);
    }
    (!token_gate.required()).then_some(requested)
}

pub async fn serve_empty_favicon() -> Response {
    (
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, immutable_cache_control())],
    )
        .into_response()
}

fn is_generated_media_path(url_path: &str) -> bool {
    let segments: Vec<&str> = url_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    matches!(
        segments.as_slice(),
        [_, namespace, _] if *namespace == forge_overlay::GENERATED_MEDIA_DIRECTORY
    )
}

fn immutable_cache_control() -> HeaderValue {
    let value = format!(
        "{CACHE_CONTROL_PUBLIC}, {CACHE_CONTROL_MAX_AGE_PARAM}={GENERATED_MEDIA_CACHE_MAX_AGE_SECONDS}, {CACHE_CONTROL_IMMUTABLE}"
    );
    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static(CACHE_CONTROL_PUBLIC))
}

fn is_overlay_config(resolved: &Path) -> bool {
    resolved.file_name().and_then(|name| name.to_str()) == Some(forge_overlay::CONFIG_FILE)
}

pub(crate) fn cors_header_value(state: &AppState, headers: &HeaderMap) -> HeaderValue {
    if state.overlay_cors_any_origin {
        return HeaderValue::from_static(ANY_ORIGIN);
    }
    scoped_cors_header_value(state, headers)
}

fn scoped_cors_header_value(state: &AppState, headers: &HeaderMap) -> HeaderValue {
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    origin
        .filter(|_| accepts_origin(&state.allowed_origins, origin, host))
        .and_then(|raw| HeaderValue::from_str(raw.trim()).ok())
        .unwrap_or_else(|| {
            let addr = format!("{SELF_SCHEME}{}", state.bind_addr);
            HeaderValue::from_str(&addr).unwrap_or_else(|_| HeaderValue::from_static(ANY_ORIGIN))
        })
}

struct ServedFile {
    file: tokio::fs::File,
    len: u64,
    path: PathBuf,
}

async fn resolve_and_open(state: &AppState, url_path: &str) -> Result<ServedFile, StatusCode> {
    let requested = crate::sandbox::request_relative_path(url_path).ok_or(StatusCode::NOT_FOUND)?;
    let canon_root = tokio::fs::canonicalize(state.overlay_root.as_ref())
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let canon_target = crate::sandbox::confine_within(&canon_root, &requested)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let meta = tokio::fs::metadata(&canon_target)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let canon_file = if meta.is_dir() {
        let entry_relative = requested.join(OVERLAY_ENTRY_DOCUMENT);
        let canon_entry = crate::sandbox::confine_within(&canon_root, &entry_relative)
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

    if !overlay_serving_enabled(state, &canon_root, &canon_file).await {
        return Err(StatusCode::NOT_FOUND);
    }

    let file = tokio::fs::File::open(&canon_file)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let len = file
        .metadata()
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?
        .len();
    Ok(ServedFile {
        file,
        len,
        path: canon_file,
    })
}

async fn overlay_serving_enabled(state: &AppState, canon_root: &Path, canon_file: &Path) -> bool {
    let Some(identity) = canon_file
        .strip_prefix(canon_root)
        .ok()
        .and_then(|relative| relative.components().next())
        .and_then(|first| first.as_os_str().to_str())
    else {
        return false;
    };

    match state.overlays.get(&OverlayId::new(identity)).await {
        Ok(Some(definition)) => definition.enabled,
        Ok(None) => true,
        Err(error) => {
            tracing::warn!(%identity, %error, "overlay enabled lookup failed; refusing to serve");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_registry::SubActionRegistry;
    use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry, spawn_action_engine};
    use forge_storage::{
        CredentialId, CredentialsRepo, DataProvider, GlobalsRepo, MockOverlayRepo, OverlayConfig,
        OverlayDefinition, OverlayId, OverlayRepo, StorageError, UserGlobalsRepo,
    };
    use forge_types::OverlayCredential;
    use time::OffsetDateTime;
    use tokio::net::TcpListener;

    use crate::ServerHandle;
    use crate::auth::AuthState;
    use crate::bus_adapter::BusAdapter;
    use crate::server::AppState;
    use crate::server_info::ServerInfo;
    use crate::test_helpers::{log_capture, test_dp};

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

    const FIXTURE_BIND_ADDR: &str = "127.0.0.1:9515";
    const FIXTURE_OVERLAY_TOKEN: &str = "fgo_fixture";
    const FOREIGN_ORIGIN: &str = "https://evil.example";

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
        creds
            .store(
                &CredentialId::new(forge_storage::SERVER_OVERLAY_TOKEN_CREDENTIAL_ID),
                FIXTURE_OVERLAY_TOKEN,
            )
            .await
            .expect("seed overlay token");
        let auth = AuthState::load(false, &*creds).await.expect("auth load");
        let creds_dyn: Arc<dyn CredentialsRepo> = creds;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let bus_adapter = BusAdapter::new(bus.clone());
        bus_adapter.spawn();
        let dp: Arc<dyn DataProvider> = test_dp();
        let actions = dp.action_repo();
        let globals: Arc<dyn GlobalsRepo> = Arc::clone(&dp) as Arc<dyn GlobalsRepo>;
        let user_globals: Arc<dyn UserGlobalsRepo> = Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>;
        let overlays = overlay_repo.unwrap_or_else(|| dp.overlay_repo());
        let _registry = Arc::new(ScriptRegistry::new());
        let action_engine = Arc::new(spawn_action_engine(
            Arc::clone(&bus),
            forge_runtime::Catalog::new(
                dp.action_repo(),
                dp.trigger_instance_repo(),
                dp.catalog_revision(),
            ),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(SubActionRegistry::new()),
            Arc::new(forge_runtime::ActionCancelRegistry::new()),
        ));
        let bind_addr: SocketAddr = FIXTURE_BIND_ADDR.parse().expect("addr");
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
            server_info: ServerInfo::serving(std::collections::BTreeSet::new()),
            action_engine,
            audio_clips: crate::audio_clips::AudioClipStore::new(),
            overlay_root: Arc::new(overlay_root),
            overlay_cors_any_origin,
            bind_addr,
            timing: crate::ServerTiming::default(),
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

        let resp = reqwest::get(format!("http://{}/overlays/fgo_fixture/alerts.html", addr))
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 200);
        let ct = resp.headers().get("content-type").expect("content-type");
        assert!(ct.to_str().unwrap().contains("text/html"));
        let body = resp.bytes().await.expect("body");
        assert_eq!(&body[..], b"<h1>Alert</h1>");

        handle.stop().await.expect("stop");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_overlay_written_owner_only_is_served_whole_over_http() {
        use std::os::unix::fs::PermissionsExt as _;

        const OWNER_ONLY: u32 = 0o600;
        const PERMISSION_BITS: u32 = 0o777;
        const IDENTITY: &str = "sub-alert-1";
        const CLIP: &[u8] = b"RIFF\x04\x00\x00\x00WAVE";
        const CLIP_IDENTITY: &str =
            "sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8";

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("overlays");
        let mut kinds = forge_overlay::OverlayKindRegistry::new();
        forge_overlay::register_builtin_kinds(&mut kinds).expect("the builtin kinds register");
        let shared = forge_overlay::ensure_shared_directory(&root).expect("shared directory");
        let clip = forge_overlay::ResolvedMedia::new(
            forge_overlay::config::SOUND,
            CLIP_IDENTITY,
            "wav",
            CLIP.to_vec(),
        )
        .expect("a content id is a safe token");
        let report = forge_overlay::materialize_overlay(
            &root,
            &forge_overlay::OverlayInstance {
                id: IDENTITY.to_owned(),
                display_name: "Sub alert".to_owned(),
                kind_id: "overlay.alert".to_owned(),
                config: forge_overlay::OverlayConfig::new(),
                source_overrides: Vec::new().into_iter().collect(),
                credential: None,
                media: forge_overlay::OverlayMedia {
                    resolved: vec![clip],
                    issues: Vec::new(),
                },
                sample: forge_overlay::SampleContext::neutral(),
            },
            &kinds,
        )
        .expect("materialize");
        let canon_root = root.canonicalize().expect("canonical root");
        let files = [
            report.directory.join(forge_overlay::MARKUP_FILE),
            report.directory.join(forge_overlay::STYLE_FILE),
            report.directory.join(forge_overlay::BEHAVIOR_FILE),
            report.directory.join(forge_overlay::CONFIG_FILE),
            report.directory.join(forge_overlay::SAMPLE_FILE),
            report
                .directory
                .join(forge_overlay::GENERATED_MEDIA_DIRECTORY)
                .join(format!("{CLIP_IDENTITY}.wav")),
            shared.join(forge_overlay::RUNTIME_ASSET),
            shared.join(forge_overlay::MOTION_ASSET),
        ];

        let (handle, addr) = make_overlay_server(root.clone(), true, MemCreds::new()).await;

        for file in files {
            let mode = std::fs::metadata(&file)
                .expect("the generated file exists")
                .permissions()
                .mode();
            assert_eq!(mode & PERMISSION_BITS, OWNER_ONLY, "{file:?}");
            let relative = file
                .strip_prefix(&canon_root)
                .expect("under the root")
                .display();

            let resp = reqwest::get(format!("http://{addr}/overlays/fgo_fixture/{relative}"))
                .await
                .expect("request");

            assert_eq!(resp.status(), reqwest::StatusCode::OK, "{relative}");
            assert_eq!(
                resp.bytes().await.expect("body")[..],
                std::fs::read(&file).expect("the file reads back")[..],
                "{relative}"
            );
        }

        let page = reqwest::get(format!("http://{addr}/overlays/fgo_fixture/{IDENTITY}/"))
            .await
            .expect("request");
        assert_eq!(page.status(), reqwest::StatusCode::OK);

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn serve_overlay_returns_404_for_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");

        let (handle, addr) =
            make_overlay_server(dir.path().to_path_buf(), true, MemCreds::new()).await;

        let resp = reqwest::get(format!("http://{}/overlays/fgo_fixture/nope.html", addr))
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 404);

        handle.stop().await.expect("stop");
    }

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
            "/overlays/fgo_fixture/%2e%2e%2fsecret.html",
            "/overlays/fgo_fixture/..%2fsecret.html",
            "/overlays/fgo_fixture/%2E%2E%2Fsecret.html",
            "/overlays/fgo_fixture/sub%2f%2e%2e%2f%2e%2e%2fsecret.html",
            "/overlays/fgo_fixture/.%2e%2fsecret.html",
        ] {
            let (status, body) = raw_get(addr, target, "").await;
            assert_eq!(status, 404, "expected 404 for {target}");
            assert!(
                !body.contains(ESCAPE_MARKER),
                "escaped the overlay root via {target}"
            );
        }

        handle.stop().await.expect("stop");
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
            format!("/overlays/fgo_fixture/{encoded}"),
            format!("/overlays/fgo_fixture/{absolute}"),
        ] {
            let (status, body) = raw_get(addr, &target, "").await;
            assert_eq!(status, 404, "expected 404 for {target}");
            assert!(
                !body.contains(ESCAPE_MARKER),
                "absolute path re-rooted the join via {target}"
            );
        }

        handle.stop().await.expect("stop");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn serve_overlay_rejects_symlink_pointing_outside_the_root() {
        let dir = qa_tempdir();
        let (root, decoy) = root_with_outside_decoy(dir.path()).await;
        std::os::unix::fs::symlink(dir.path().join(decoy), root.join("link.html"))
            .expect("symlink");

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        let (status, body) = raw_get(addr, "/overlays/fgo_fixture/link.html", "").await;
        assert_eq!(status, 404);
        assert!(!body.contains(ESCAPE_MARKER));

        handle.stop().await.expect("stop");
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

        let (status, body) = raw_get(addr, "/overlays/fgo_fixture/alias.html", "").await;
        assert_eq!(status, 200);
        assert!(body.contains("<p>inside</p>"));

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn serve_overlay_returns_404_for_hidden_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join(".secret"), b"hidden")
            .await
            .expect("write");

        let (handle, addr) =
            make_overlay_server(dir.path().to_path_buf(), true, MemCreds::new()).await;

        let resp = reqwest::get(format!("http://{}/overlays/fgo_fixture/.secret", addr))
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 404);

        handle.stop().await.expect("stop");
    }

    const WIDGET_FILE: &str = "widget.js";
    const ANY_ORIGIN_WILDCARD: &str = "*";

    async fn overlay_response_headers<F>(
        any_origin: bool,
        origin_for: F,
    ) -> (SocketAddr, reqwest::header::HeaderMap)
    where
        F: FnOnce(SocketAddr) -> Option<String>,
    {
        let dir = qa_tempdir();
        tokio::fs::write(dir.path().join(WIDGET_FILE), b"console.log('hi')")
            .await
            .expect("write");
        let (handle, addr) =
            make_overlay_server(dir.path().to_path_buf(), any_origin, MemCreds::new()).await;

        let mut request =
            reqwest::Client::new().get(format!("http://{addr}/overlays/fgo_fixture/{WIDGET_FILE}"));
        if let Some(origin) = origin_for(addr) {
            request = request.header(reqwest::header::ORIGIN, origin);
        }
        let response = request.send().await.expect("request");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let headers = response.headers().clone();

        handle.stop().await.expect("stop");
        (addr, headers)
    }

    fn header_text(
        headers: &reqwest::header::HeaderMap,
        name: reqwest::header::HeaderName,
    ) -> String {
        headers
            .get(&name)
            .expect("response carries the header under test")
            .to_str()
            .expect("header value is ascii")
            .to_owned()
    }

    fn allowed_origin(headers: &reqwest::header::HeaderMap) -> String {
        header_text(headers, reqwest::header::ACCESS_CONTROL_ALLOW_ORIGIN)
    }

    #[tokio::test]
    async fn serve_overlay_returns_cors_star_when_any_origin_true() {
        let (_, headers) = overlay_response_headers(true, |_| None).await;

        assert_eq!(allowed_origin(&headers), ANY_ORIGIN_WILDCARD);
    }

    #[tokio::test]
    async fn serve_overlay_echoes_an_origin_the_handshake_rule_accepts() {
        let (addr, headers) =
            overlay_response_headers(false, |addr| Some(format!("http://{addr}"))).await;

        assert_eq!(allowed_origin(&headers), format!("http://{addr}"));
    }

    #[tokio::test]
    async fn serve_overlay_never_echoes_an_origin_the_handshake_rule_rejects() {
        let (_, headers) =
            overlay_response_headers(false, |_| Some(FOREIGN_ORIGIN.to_owned())).await;

        assert_eq!(
            allowed_origin(&headers),
            format!("http://{FIXTURE_BIND_ADDR}")
        );
    }

    #[tokio::test]
    async fn serve_overlay_falls_back_to_the_bind_authority_when_no_origin_is_present() {
        let (_, headers) = overlay_response_headers(false, |_| None).await;

        assert_eq!(
            allowed_origin(&headers),
            format!("http://{FIXTURE_BIND_ADDR}")
        );
    }

    #[tokio::test]
    async fn serve_overlay_marks_the_answer_as_varying_on_origin_in_either_cors_mode() {
        for any_origin in [true, false] {
            let (_, headers) = overlay_response_headers(any_origin, |_| None).await;

            assert!(
                header_text(&headers, reqwest::header::VARY)
                    .eq_ignore_ascii_case(reqwest::header::ORIGIN.as_str()),
                "any_origin={any_origin}"
            );
        }
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

        for target in [
            "/overlays/fgo_fixture/alerts/",
            "/overlays/fgo_fixture/alerts",
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
                content_type.contains("text/html"),
                "index document typed as {content_type} for {target}"
            );
            let body = resp.text().await.expect("body");
            assert_eq!(body, "<h1>Entry</h1>", "wrong body for {target}");
        }

        handle.stop().await.expect("stop");
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
            "/overlays/fgo_fixture/bare/",
            "/overlays/fgo_fixture/bare",
            "/overlays/fgo_fixture/shadowed/",
            "/overlays/fgo_fixture/shadowed",
        ] {
            let (status, _) = get_status_and_body(addr, target).await;
            assert_eq!(status, 404, "expected 404 for {target}");
        }

        handle.stop().await.expect("stop");
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

        for target in [
            "/overlays/fgo_fixture/alerts/",
            "/overlays/fgo_fixture/alerts",
        ] {
            let (status, body) = raw_get(addr, target, "").await;
            assert_eq!(status, 404, "expected 404 for {target}");
            assert!(
                !body.contains(ESCAPE_MARKER),
                "directory index escaped the overlay root via {target}"
            );
        }

        handle.stop().await.expect("stop");
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
            ("/overlays/fgo_fixture/config.json", "application/json"),
            ("/overlays/fgo_fixture/face.woff2", "font/woff2"),
            (
                "/overlays/fgo_fixture/blob.unknownext",
                "application/octet-stream",
            ),
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

        handle.stop().await.expect("stop");
    }

    const MEDIA_FILE: &str =
        "sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8.wav";
    const CACHEABLE_FLOOR_SECONDS: u64 = 30 * 24 * 60 * 60;

    async fn media_overlay_root(base: &std::path::Path) -> std::path::PathBuf {
        let root = make_root(base).await;
        let overlay = root.join("alerts");
        write_at(overlay.join("index.html"), "<h1>Entry</h1>").await;
        write_at(overlay.join("config.json"), "{}").await;
        write_at(overlay.join(MEDIA_FILE), "hand placed").await;
        write_at(
            overlay
                .join(forge_overlay::GENERATED_MEDIA_DIRECTORY)
                .join(MEDIA_FILE),
            "generated",
        )
        .await;
        write_at(
            overlay
                .join(forge_overlay::GENERATED_MEDIA_DIRECTORY)
                .join("nested")
                .join(MEDIA_FILE),
            "deeper",
        )
        .await;
        write_at(
            overlay
                .join(format!("{}-user", forge_overlay::GENERATED_MEDIA_DIRECTORY))
                .join(MEDIA_FILE),
            "lookalike",
        )
        .await;
        root
    }

    async fn cache_control_of(addr: SocketAddr, target: &str) -> Option<String> {
        let response = reqwest::get(format!("http://{addr}{target}"))
            .await
            .expect("request");
        assert_eq!(response.status().as_u16(), 200, "expected 200 for {target}");
        response
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .map(|value| {
                value
                    .to_str()
                    .expect("an ascii cache-control value")
                    .to_owned()
            })
    }

    #[tokio::test]
    async fn only_a_content_named_file_in_the_generated_namespace_is_cached_forever() {
        let dir = qa_tempdir();
        let root = media_overlay_root(dir.path()).await;

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        let namespace = forge_overlay::GENERATED_MEDIA_DIRECTORY;
        for target in [
            "/overlays/fgo_fixture/alerts".to_owned(),
            "/overlays/fgo_fixture/alerts/".to_owned(),
            "/overlays/fgo_fixture/alerts/index.html".to_owned(),
            "/overlays/fgo_fixture/alerts/config.json".to_owned(),
            format!("/overlays/fgo_fixture/alerts/{MEDIA_FILE}"),
            format!("/overlays/fgo_fixture/alerts/{namespace}/nested/{MEDIA_FILE}"),
            format!("/overlays/fgo_fixture/alerts/{namespace}-user/{MEDIA_FILE}"),
        ] {
            assert_eq!(
                cache_control_of(addr, &target).await,
                None,
                "{target} was handed an immutable cache directive"
            );
        }

        let cached = cache_control_of(
            addr,
            &format!("/overlays/fgo_fixture/alerts/{namespace}/{MEDIA_FILE}"),
        )
        .await
        .expect("a content-named generated file carries a cache directive");
        assert!(cached.contains("public"), "{cached}");
        assert!(cached.contains("immutable"), "{cached}");
        let max_age: u64 = cached
            .split(',')
            .filter_map(|part| part.trim().strip_prefix("max-age="))
            .next()
            .expect("the directive states a max age")
            .parse()
            .expect("the max age is a number of seconds");
        assert!(
            max_age >= CACHEABLE_FLOOR_SECONDS,
            "a content-named body was given a {max_age}s lifetime"
        );

        handle.stop().await.expect("stop");
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
            "/overlays/fgo_fixture/off/",
            "/overlays/fgo_fixture/off",
            "/overlays/fgo_fixture/off/index.html",
            "/overlays/fgo_fixture/off/asset.js",
        ] {
            let (status, body) = get_status_and_body(addr, target).await;
            assert_eq!(status, 404, "expected 404 for {target}");
            assert!(
                !body.contains("<p>off</p>"),
                "disabled overlay served its body via {target}"
            );
        }

        handle.stop().await.expect("stop");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn serve_overlay_gates_on_the_identity_the_request_resolves_into_not_the_one_it_names() {
        let dir = qa_tempdir();
        let root = gated_overlay_root(dir.path()).await;
        std::os::unix::fs::symlink("off", root.join("alias")).expect("symlink");

        let (handle, addr) = make_overlay_server_with_overlays(
            root,
            true,
            MemCreds::new(),
            Some(gated_overlay_repo()),
        )
        .await;

        for target in [
            "/overlays/fgo_fixture/alias/index.html",
            "/overlays/fgo_fixture/alias/asset.js",
            "/overlays/fgo_fixture/alias/",
        ] {
            let (status, body) = get_status_and_body(addr, target).await;
            assert_eq!(status, 404, "expected 404 for {target}");
            assert!(
                !body.contains("<p>off</p>"),
                "a disabled overlay served itself under an alias via {target}"
            );
        }

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn serve_overlay_refuses_a_page_whose_enabled_state_the_store_cannot_report() {
        let dir = qa_tempdir();
        let root = gated_overlay_root(dir.path()).await;
        let (handle, addr) = make_overlay_server_with_overlays(
            root,
            true,
            MemCreds::new(),
            Some(gated_overlay_repo()),
        )
        .await;

        let (status, _) = get_status_and_body(addr, "/overlays/fgo_fixture/boom/index.html").await;

        assert_eq!(status, 404);
        handle.stop().await.expect("stop");
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
            ("/overlays/fgo_fixture/on/index.html", "<p>on</p>"),
            (
                "/overlays/fgo_fixture/stranger/index.html",
                "<p>stranger</p>",
            ),
            (
                "/overlays/fgo_fixture/forge-shared/runtime-v1.js",
                "export const runtime = 1;",
            ),
        ] {
            let (status, body) = get_status_and_body(addr, target).await;
            assert_eq!(status, 200, "expected 200 for {target}");
            assert_eq!(body, expected_body, "wrong body for {target}");
        }

        handle.stop().await.expect("stop");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn serve_overlay_refuses_backslash_colon_and_unc_shaped_paths() {
        let dir = qa_tempdir();
        let root = make_root(dir.path()).await;
        write_at(root.join("a\\b.html"), ESCAPE_MARKER).await;
        write_at(root.join("a:b.html"), ESCAPE_MARKER).await;

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        for target in [
            "/overlays/fgo_fixture/a%5Cb.html",
            "/overlays/fgo_fixture/a:b.html",
            "/overlays/fgo_fixture/a%3Ab.html",
            "/overlays/fgo_fixture/%5C%5Chost%5Cs%5Cx",
        ] {
            let (status, body) = raw_get(addr, target, "").await;
            assert_eq!(status, 404, "expected 404 for {target}");
            assert!(!body.contains(ESCAPE_MARKER), "{target} was served");
        }

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn serve_overlay_withholds_the_wildcard_from_config_json_only() {
        let dir = qa_tempdir();
        let root = make_root(dir.path()).await;
        write_at(root.join("alerts").join(forge_overlay::CONFIG_FILE), "{}").await;
        write_at(root.join("alerts").join(WIDGET_FILE), "console.log('hi')").await;

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        let mut granted = Vec::new();
        for file in [forge_overlay::CONFIG_FILE, WIDGET_FILE] {
            let response = reqwest::Client::new()
                .get(format!("http://{addr}/overlays/fgo_fixture/alerts/{file}"))
                .header(reqwest::header::ORIGIN, FOREIGN_ORIGIN)
                .send()
                .await
                .expect("request");
            assert_eq!(response.status(), reqwest::StatusCode::OK, "{file}");
            granted.push(allowed_origin(response.headers()));
        }

        assert_eq!(
            granted,
            [
                format!("http://{FIXTURE_BIND_ADDR}"),
                ANY_ORIGIN_WILDCARD.to_owned()
            ]
        );
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn serve_overlay_streams_a_multi_chunk_file_whole_under_its_exact_length() {
        const LARGE_FILE_BYTES: usize = 100 * 1024 + 7;
        let dir = qa_tempdir();
        let root = make_root(dir.path()).await;
        let content: Vec<u8> = (0..LARGE_FILE_BYTES)
            .map(|n| u8::try_from(n % 251).expect("below 251"))
            .collect();
        tokio::fs::write(root.join("clip.wav"), &content)
            .await
            .expect("write");

        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        let response = reqwest::get(format!("http://{addr}/overlays/fgo_fixture/clip.wav"))
            .await
            .expect("request");
        assert_eq!(
            header_text(response.headers(), reqwest::header::CONTENT_LENGTH),
            LARGE_FILE_BYTES.to_string()
        );
        let body = response.bytes().await.expect("body");
        assert!(
            body[..] == content[..],
            "the streamed body differs from the file"
        );

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn the_favicon_request_is_answered_with_no_content_and_cached_forever() {
        let dir = qa_tempdir();
        let (handle, addr) =
            make_overlay_server(dir.path().to_path_buf(), true, MemCreds::new()).await;

        let response = reqwest::get(format!("http://{addr}{}", super::FAVICON_PATH))
            .await
            .expect("request");

        assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);
        assert_eq!(
            response.headers().get(reqwest::header::CACHE_CONTROL),
            Some(&super::immutable_cache_control()),
            "a browser source must not ask for the icon again"
        );
        assert!(response.bytes().await.expect("body").is_empty());

        handle.stop().await.expect("stop");
    }

    const OVERLAY_PAGE_PATHS: &[&str] = &[
        "alerts/",
        "alerts/index.html",
        "alerts/config.json",
        "alerts/overlay.css",
        "alerts/media/chime.ogg",
        "forge-shared/runtime-v1.js",
        "loose.html",
    ];

    async fn overlay_page_root(base: &std::path::Path) -> std::path::PathBuf {
        let root = make_root(base).await;
        write_at(root.join("alerts").join("index.html"), "<p>entry</p>").await;
        write_at(
            root.join("alerts").join("config.json"),
            "{\"credential\":\"c\"}",
        )
        .await;
        write_at(root.join("alerts").join("overlay.css"), "body{}").await;
        write_at(root.join("alerts").join("media").join("chime.ogg"), "ogg").await;
        write_at(
            root.join("forge-shared").join("runtime-v1.js"),
            "export const runtime = 1;",
        )
        .await;
        write_at(root.join("loose.html"), "<p>loose</p>").await;
        root
    }

    fn referrer_policy(response: &reqwest::Response) -> Option<String> {
        response
            .headers()
            .get(reqwest::header::REFERRER_POLICY)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }

    #[tokio::test]
    async fn every_overlay_path_without_the_exact_token_is_refused_with_an_empty_no_referrer_404() {
        let dir = qa_tempdir();
        let root = overlay_page_root(dir.path()).await;
        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        for prefix in [
            "",
            "fgo_wrong/",
            "fgo_fixtur/",
            "fgo_fixturee/",
            "FGO_FIXTURE/",
            "fg_fixture/",
            "/",
        ] {
            for path in OVERLAY_PAGE_PATHS {
                let target = format!("http://{addr}/overlays/{prefix}{path}");
                let response = reqwest::get(&target).await.expect("request");

                assert_eq!(response.status().as_u16(), 404, "{target}");
                assert_eq!(
                    referrer_policy(&response).as_deref(),
                    Some("no-referrer"),
                    "{target}"
                );
                assert!(
                    response.bytes().await.expect("body").is_empty(),
                    "{target} leaked a body"
                );
            }
        }

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn the_overlay_token_opens_every_file_an_overlay_page_loads_under_no_referrer() {
        let dir = qa_tempdir();
        let root = overlay_page_root(dir.path()).await;
        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        for path in OVERLAY_PAGE_PATHS {
            let target = format!("http://{addr}/overlays/{FIXTURE_OVERLAY_TOKEN}/{path}");
            let response = reqwest::get(&target).await.expect("request");

            assert_eq!(response.status().as_u16(), 200, "{target}");
            assert_eq!(
                referrer_policy(&response).as_deref(),
                Some("no-referrer"),
                "{target} would leak the tokenized path to third-party subresources"
            );
        }

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn the_overlay_token_segment_alone_serves_nothing() {
        let dir = qa_tempdir();
        let root = overlay_page_root(dir.path()).await;
        write_at(root.join("index.html"), "<p>root entry</p>").await;
        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        for required in [true, false] {
            handle.set_http_overlay_token_required(required).await;
            for target in [
                format!("/overlays/{FIXTURE_OVERLAY_TOKEN}"),
                format!("/overlays/{FIXTURE_OVERLAY_TOKEN}/"),
            ] {
                let (status, body) = get_status_and_body(addr, &target).await;

                assert_eq!(status, 404, "{target} with the token required {required}");
                assert!(
                    body.is_empty(),
                    "{target} with the token required {required}"
                );
            }
        }

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_dot_dot_after_the_overlay_token_cannot_reach_a_file_outside_the_root() {
        let dir = qa_tempdir();
        let (root, _) = root_with_outside_decoy(dir.path()).await;
        write_at(root.join("alerts").join("index.html"), "<p>entry</p>").await;
        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;

        for target in [
            format!("/overlays/{FIXTURE_OVERLAY_TOKEN}/../secret.html"),
            format!("/overlays/{FIXTURE_OVERLAY_TOKEN}/alerts/../../secret.html"),
            format!("/overlays/{FIXTURE_OVERLAY_TOKEN}/%2e%2e/secret.html"),
            format!("/overlays/{FIXTURE_OVERLAY_TOKEN}/..%2fsecret.html"),
            format!("/overlays/{FIXTURE_OVERLAY_TOKEN}/%2e%2e%2f..%2fsecret.html"),
        ] {
            let (status, response) = raw_get(addr, &target, "").await;

            assert_ne!(status, 200, "{target}");
            assert!(
                !response.contains(ESCAPE_MARKER),
                "{target} escaped the root"
            );
        }

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn switching_the_token_requirement_takes_effect_on_the_next_request() {
        let dir = qa_tempdir();
        let root = overlay_page_root(dir.path()).await;
        let (handle, addr) = make_overlay_server(root, true, MemCreds::new()).await;
        let tokenless = "/overlays/alerts/config.json";
        let tokened = format!("/overlays/{FIXTURE_OVERLAY_TOKEN}/alerts/config.json");

        handle.set_http_overlay_token_required(false).await;
        assert_eq!(get_status_and_body(addr, tokenless).await.0, 200);
        assert_eq!(
            get_status_and_body(addr, &tokened).await.0,
            200,
            "a copied URL must keep working while the requirement is off"
        );

        handle.set_http_overlay_token_required(true).await;
        assert_eq!(get_status_and_body(addr, tokenless).await.0, 404);

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_refused_overlay_request_never_reaches_the_overlay_store() {
        let dir = qa_tempdir();
        let root = overlay_page_root(dir.path()).await;
        let mut repo = MockOverlayRepo::new();
        repo.expect_get().times(0);
        let (handle, addr) =
            make_overlay_server_with_overlays(root, true, MemCreds::new(), Some(Arc::new(repo)))
                .await;

        let (status, _) = get_status_and_body(addr, "/overlays/fgo_wrong/alerts/").await;

        assert_eq!(status, 404);
        handle.stop().await.expect("stop");
    }

    #[test]
    fn overlay_requests_never_put_the_overlay_token_into_any_log_line() {
        let dir = qa_tempdir();
        let base = dir.path().to_path_buf();

        let ((), lines) = log_capture::capture_blocking(tracing::Level::TRACE, async move {
            let root = overlay_page_root(&base).await;
            let mut repo = MockOverlayRepo::new();
            repo.expect_get().returning(|_| {
                Err(StorageError::Connection {
                    reason: "overlay store offline".to_owned(),
                })
            });
            let (handle, addr) = make_overlay_server_with_overlays(
                root,
                true,
                MemCreds::new(),
                Some(Arc::new(repo)),
            )
            .await;
            for target in [
                format!("/overlays/{FIXTURE_OVERLAY_TOKEN}/alerts/"),
                format!("/overlays/{FIXTURE_OVERLAY_TOKEN}/alerts/missing.js"),
                format!("/overlays/{FIXTURE_OVERLAY_TOKEN}/../escape.html"),
                "/overlays/fgo_guess/alerts/".to_owned(),
            ] {
                raw_get(addr, &target, "").await;
            }
            handle.stop().await.expect("stop");
        });

        assert!(
            !log_capture::forge_warnings(&lines).is_empty(),
            "the store failure should have produced a warning to inspect"
        );
        for token in [FIXTURE_OVERLAY_TOKEN, "fgo_guess"] {
            assert!(
                !lines.iter().any(|line| line.mentions(token)),
                "a log line carries {token}"
            );
        }
    }
}
