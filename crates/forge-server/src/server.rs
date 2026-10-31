use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Json, Router, middleware};
use tokio::net::TcpListener;

use forge_events::EventObserver;
use forge_runtime::{ActionEngineHandle, EventHistory};
use forge_storage::{
    ActionRepo, CredentialsRepo, GlobalsRepo, OverlayRepo, SettingsRepo, UserGlobalsRepo,
    read_setting,
};

use crate::audio_clips::AudioClipStore;
use crate::auth::AuthState;
use crate::bus_adapter::BusAdapter;
use crate::delivery_hub::DeliveryHub;
use crate::listener::{GuardedListener, PeerInfo};
use crate::origin::{accepts_host, build_allowed_origins};
use crate::routes::{api_v1, audio, overlays, ws};
use crate::server_info::ServerInfo;
use crate::settings;
use crate::{ServerConfig, ServerError, ServerHandle, ServerTiming};

#[derive(Clone)]
pub struct AppState {
    pub(crate) auth: Arc<AuthState>,
    pub bus: Arc<dyn EventHistory>,
    pub bus_adapter: Arc<BusAdapter>,
    pub actions: Arc<dyn ActionRepo>,
    pub globals: Arc<dyn GlobalsRepo>,
    pub user_globals: Arc<dyn UserGlobalsRepo>,
    pub overlays: Arc<dyn OverlayRepo>,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub settings: Arc<dyn SettingsRepo>,
    pub server_info: Arc<ServerInfo>,
    pub action_engine: Arc<ActionEngineHandle>,
    pub audio_clips: Arc<AudioClipStore>,
    pub overlay_root: Arc<std::path::PathBuf>,
    pub overlay_cors_any_origin: bool,
    pub bind_addr: std::net::SocketAddr,
    pub allowed_origins: Arc<HashSet<String>>,
    pub timing: ServerTiming,
}

pub struct Server {
    pub config: ServerConfig,
}

impl Server {
    pub async fn start(self) -> Result<ServerHandle, ServerError> {
        let addr = self.config.bind_addr;
        validate_lan_bind(&addr, self.config.lan_bind_enabled)?;
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| ServerError::Bind {
                addr: addr.to_string(),
                reason: e.to_string(),
            })?;
        let bind_addr = listener.local_addr().unwrap_or(addr);
        let state = build_state(self.config, bind_addr).await?;
        Ok(serve_on(listener, state))
    }

    pub async fn stopped(self) -> Result<ServerHandle, ServerError> {
        let bind_addr = self.config.bind_addr;
        let state = build_state(self.config, bind_addr).await?;
        Ok(ServerHandle::stopped(state, bind_addr))
    }
}

async fn build_state(config: ServerConfig, bind_addr: SocketAddr) -> Result<AppState, ServerError> {
    let auth = AuthState::load(config.auth_required_for_reads, config.credentials.as_ref()).await?;
    auth.set_lan_exposed(is_lan_bind(bind_addr.ip()));
    auth.overlay_token().set_required(
        read_setting(
            config.settings.as_ref(),
            &settings::HTTP_OVERLAY_REQUIRE_TOKEN,
        )
        .await?,
    );
    let bus = Arc::clone(&config.event_bus);
    let observer: Arc<dyn EventObserver> = bus.clone();
    let bus_adapter = config
        .delivery_hub
        .unwrap_or_else(|| DeliveryHub::new(observer))
        .adapter();
    let allowed_origins = Arc::new(build_allowed_origins(bind_addr, &config.additional_origins));
    Ok(AppState {
        auth,
        bus,
        bus_adapter,
        actions: config.actions,
        globals: config.globals,
        user_globals: config.user_globals,
        overlays: config.overlays,
        credentials: config.credentials,
        settings: config.settings,
        server_info: ServerInfo::serving(config.available_platforms),
        action_engine: config.action_engine,
        audio_clips: AudioClipStore::new(),
        overlay_root: Arc::new(config.overlay_root),
        overlay_cors_any_origin: config.overlay_cors_any_origin,
        bind_addr,
        allowed_origins,
        timing: config.timing,
    })
}

pub async fn start_server(config: ServerConfig) -> Result<ServerHandle, ServerError> {
    Server { config }.start().await
}

pub async fn stopped_server(config: ServerConfig) -> Result<ServerHandle, ServerError> {
    Server { config }.stopped().await
}

pub(crate) fn is_lan_bind(address: IpAddr) -> bool {
    !address.to_canonical().is_loopback()
}

pub(crate) fn validate_lan_bind(
    addr: &SocketAddr,
    lan_bind_enabled: bool,
) -> Result<(), ServerError> {
    if is_lan_bind(addr.ip()) && !lan_bind_enabled {
        return Err(ServerError::LanBindNotEnabled {
            addr: addr.to_string(),
        });
    }
    Ok(())
}

async fn metrics_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    state.server_info.record_http_request();
    next.run(request).await
}

async fn auth_middleware(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let is_mutating = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );

    if is_mutating || state.auth.reads_required() {
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

async fn host_middleware(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .map(|value| value.to_str().ok());
    let accepted = match host {
        None => true,
        Some(host) => host.is_some_and(|host| accepts_host(&state.allowed_origins, host)),
    };
    if !accepted {
        return misdirected_response();
    }
    next.run(request).await
}

fn misdirected_response() -> Response {
    (
        StatusCode::MISDIRECTED_REQUEST,
        Json(serde_json::json!({
            "error": {
                "code": "HOST_NOT_ALLOWED",
                "message": "Host not allowed; open the server by IP address or add the origin in Settings"
            }
        })),
    )
        .into_response()
}

fn build_router(state: AppState, shutdown: tokio::sync::watch::Receiver<bool>) -> Router {
    let host_guard = middleware::from_fn_with_state(state.clone(), host_middleware);
    let api_routes = api_v1::router()
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .route_layer(host_guard.clone());

    Router::new()
        .route("/ws/v1/", get(ws::ws_handler).layer(Extension(shutdown)))
        .nest("/api/v1", api_routes)
        .route(
            &format!("/{}/{{*path}}", crate::overlay_url::OVERLAY_ROUTE),
            get(overlays::serve_overlay_file).route_layer(host_guard.clone()),
        )
        .route(
            overlays::FAVICON_PATH,
            get(overlays::serve_empty_favicon).route_layer(host_guard.clone()),
        )
        .merge(audio::router().route_layer(host_guard))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            metrics_middleware,
        ))
        .with_state(state)
}

pub fn serve_on_with_shutdown(
    listener: TcpListener,
    state: AppState,
    run_state_tx: tokio::sync::watch::Sender<bool>,
) -> (
    tokio::task::JoinHandle<Result<(), ServerError>>,
    tokio::sync::watch::Sender<bool>,
) {
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let read_timeout = state.timing.request_read_timeout;
    let app =
        build_router(state, shutdown_rx.clone()).into_make_service_with_connect_info::<PeerInfo>();
    let listener = GuardedListener::new(listener).with_read_timeout(read_timeout);
    let join = tokio::spawn(async move {
        let result = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                loop {
                    if shutdown_rx.changed().await.is_err() {
                        break;
                    }
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
            })
            .await
            .map_err(|e| ServerError::Io(std::io::Error::other(e)));
        run_state_tx.send_replace(false);
        result
    });
    (join, shutdown_tx)
}

fn serve_on(listener: TcpListener, state: AppState) -> ServerHandle {
    let bind_addr = listener.local_addr().unwrap_or(state.bind_addr);
    let stored_state = state.clone();
    let (run_state_tx, _run_state_rx) = tokio::sync::watch::channel(true);
    let (reported_tx, reported_rx) = tokio::sync::watch::channel(true);
    let (join, shutdown_tx) = serve_on_with_shutdown(listener, state, reported_tx);
    let handle = ServerHandle::new(join, shutdown_tx, stored_state, bind_addr, run_state_tx);
    handle.adopt_generation(reported_rx);
    handle
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_runtime::{EventBus, NullEventLogRepo};
    use forge_storage::{CredentialsRepo, SettingsRepo};
    use time::OffsetDateTime;
    use tokio::net::TcpListener;
    use tracing::Level;

    use super::{
        AppState, AuthState, BusAdapter, ServerConfig, ServerHandle, build_state, is_lan_bind,
        serve_on, start_server, stopped_server, validate_lan_bind,
    };
    use crate::ServerError;
    use crate::audio_clips::{ClipMediaType, ClipOffer, ClipOutcome};
    use crate::bus_adapter::{ClientFilterSet, ClientId, EventFilter, WsFrame};
    use crate::test_helpers::{
        MemCreds, app_state, log_capture, mem_settings, server_config, test_dp,
    };
    use crate::ws_client::WsClient;
    use forge_runtime::OverlayFrameSink;
    use forge_storage::test_support::MemorySettings;

    async fn make_server(
        auth_required_for_reads: bool,
        creds: Arc<MemCreds>,
    ) -> (ServerHandle, std::net::SocketAddr) {
        let auth = AuthState::load(auth_required_for_reads, &*creds)
            .await
            .expect("auth load");
        let creds_dyn: Arc<dyn CredentialsRepo> = creds;
        let state = app_state(auth, creds_dyn);
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
        let state = app_state(auth, creds_dyn);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let handle = serve_on(listener, state);
        (handle, addr, auth_ref)
    }

    async fn make_server_exposing_state() -> (ServerHandle, AppState) {
        let creds = MemCreds::new();
        let auth = AuthState::load(false, &*creds).await.expect("auth load");
        let creds_dyn: Arc<dyn CredentialsRepo> = creds;
        let state = app_state(auth, creds_dyn);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let observed = state.clone();
        (serve_on(listener, state), observed)
    }

    async fn attach_client(
        state: &AppState,
        identification: &str,
    ) -> (ClientId, tokio::sync::broadcast::Receiver<WsFrame>) {
        let filters = ClientFilterSet::new(std::collections::HashSet::from([EventFilter::new(
            None, None,
        )]));
        let (client_handle, rx) = state.bus_adapter.register_client(filters).await;
        let client = Arc::new(WsClient::new(
            client_handle.id,
            "203.0.113.10:5555".parse().expect("addr"),
            Arc::clone(&client_handle.drop_counter),
        ));
        client
            .identification
            .store(Arc::new(identification.to_owned()));
        state.server_info.register(client_handle.id, client).await;
        (client_handle.id, rx)
    }

    async fn make_server_targeting_a_free_port() -> (ServerHandle, u16) {
        let settings = mem_settings();
        let creds = MemCreds::with_token("my-token");
        let auth = AuthState::load(false, &*creds).await.expect("auth load");
        let creds_dyn: Arc<dyn CredentialsRepo> = creds;
        let state = AppState {
            settings: Arc::clone(&settings) as Arc<dyn SettingsRepo>,
            ..app_state(auth, creds_dyn)
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let boot_addr = listener.local_addr().expect("local addr");
        let handle = serve_on(listener, state);

        let new_port = reserve_a_free_port().await;
        assert_ne!(
            boot_addr.port(),
            new_port,
            "target port must differ from boot"
        );

        crate::config::ServerSettings::save_bind_address(&*settings, "127.0.0.1")
            .await
            .expect("save addr");
        crate::config::ServerSettings::save_port(&*settings, new_port)
            .await
            .expect("save port");

        (handle, new_port)
    }

    const RESERVED_PORT_FLOOR: u16 = 20_000;
    const RESERVED_PORT_SPAN: u16 = 10_000;
    const PROCESS_PORT_STRIDE: u64 = 1_009;

    async fn reserve_a_free_port() -> u16 {
        static NEXT_OFFSET: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
        loop {
            let next = NEXT_OFFSET.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let process_start = u64::from(std::process::id()) * PROCESS_PORT_STRIDE;
            let offset = (u64::from(next) + process_start) % u64::from(RESERVED_PORT_SPAN);
            let port = RESERVED_PORT_FLOOR + u16::try_from(offset).unwrap();
            if TcpListener::bind(format!("{LOOPBACK}:{port}"))
                .await
                .is_ok()
            {
                return port;
            }
        }
    }

    fn loopback_config(
        settings: Arc<MemorySettings>,
        port: u16,
        additional_origins: Vec<String>,
    ) -> ServerConfig {
        let mut config = server_config(
            settings as Arc<dyn SettingsRepo>,
            MemCreds::new(),
            test_dp(),
        );
        config.bind_addr = format!("127.0.0.1:{port}").parse().expect("addr");
        config.overlay_root = std::path::PathBuf::from("/tmp/forge-test-overlays");
        config.additional_origins = additional_origins;
        config
    }

    #[tokio::test]
    async fn a_failed_read_of_the_overlay_token_switch_refuses_to_build_a_server() {
        let settings = mem_settings();
        settings.fail_reads_for(crate::settings::HTTP_OVERLAY_REQUIRE_TOKEN.name());
        let started = start_server(loopback_config(
            Arc::clone(&settings),
            EPHEMERAL_PORT,
            Vec::new(),
        ))
        .await;
        let stopped = stopped_server(loopback_config(settings, EPHEMERAL_PORT, Vec::new())).await;

        assert!(matches!(started, Err(ServerError::Storage(_))));
        assert!(matches!(stopped, Err(ServerError::Storage(_))));
    }

    async fn start_on_an_ephemeral_port(
        settings: Arc<MemorySettings>,
        additional_origins: Vec<String>,
    ) -> (ServerHandle, std::net::SocketAddr) {
        start_with_timing(settings, additional_origins, crate::ServerTiming::default()).await
    }

    async fn start_with_timing(
        settings: Arc<MemorySettings>,
        additional_origins: Vec<String>,
        timing: crate::ServerTiming,
    ) -> (ServerHandle, std::net::SocketAddr) {
        let mut config = loopback_config(settings, EPHEMERAL_PORT, additional_origins);
        config.timing = timing;
        let handle = start_server(config).await.expect("start");
        let addr = handle.bind_addr().await;
        (handle, addr)
    }

    async fn handle_awaiting_a_port(
        settings: Arc<MemorySettings>,
        listening: bool,
    ) -> (ServerHandle, u16) {
        let port = reserve_a_free_port().await;
        crate::config::ServerSettings::save_bind_address(&*settings, LOOPBACK)
            .await
            .expect("save addr");
        crate::config::ServerSettings::save_port(&*settings, port)
            .await
            .expect("save port");
        let config = loopback_config(settings, port, Vec::new());
        let handle = if listening {
            start_server(config).await.expect("listening handle")
        } else {
            stopped_server(config).await.expect("stopped handle")
        };
        (handle, port)
    }

    async fn stopped_awaiting_a_port(settings: Arc<MemorySettings>) -> (ServerHandle, u16) {
        handle_awaiting_a_port(settings, false).await
    }

    async fn accepts_on(port: u16) -> bool {
        tokio::net::TcpStream::connect(format!("{LOOPBACK}:{port}"))
            .await
            .is_ok()
    }

    async fn info_status(port: u16) -> u16 {
        reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/api/v1/info"))
            .send()
            .await
            .expect("HTTP request")
            .status()
            .as_u16()
    }

    async fn ws_handshake(
        addr: std::net::SocketAddr,
        origin: Option<&str>,
    ) -> Result<(), tokio_tungstenite::tungstenite::Error> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let mut request = format!("ws://{addr}/ws/v1/")
            .into_client_request()
            .expect("client request");
        if let Some(origin) = origin {
            request.headers_mut().insert(
                axum::http::header::ORIGIN,
                origin.parse().expect("origin header value"),
            );
        }
        tokio_tungstenite::connect_async(request).await.map(|_| ())
    }

    async fn ws_handshake_presenting_host(
        addr: std::net::SocketAddr,
        origin: &str,
        host: &str,
    ) -> Result<(), tokio_tungstenite::tungstenite::Error> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let mut request = format!("ws://{addr}/ws/v1/")
            .into_client_request()
            .expect("client request");
        let headers = request.headers_mut();
        headers.insert(
            axum::http::header::HOST,
            host.parse().expect("host header value"),
        );
        headers.insert(
            axum::http::header::ORIGIN,
            origin.parse().expect("origin header value"),
        );
        tokio_tungstenite::connect_async(request).await.map(|_| ())
    }

    fn as_http_refusal(
        error: tokio_tungstenite::tungstenite::Error,
    ) -> tokio_tungstenite::tungstenite::http::Response<Option<Vec<u8>>> {
        match error {
            tokio_tungstenite::tungstenite::Error::Http(response) => Some(*response),
            _ => None,
        }
        .expect("refusal must arrive as an HTTP response, not a transport error")
    }

    async fn refused_ws_handshake(
        addr: std::net::SocketAddr,
        origin: &str,
    ) -> tokio_tungstenite::tungstenite::http::Response<Option<Vec<u8>>> {
        as_http_refusal(
            ws_handshake(addr, Some(origin))
                .await
                .expect_err("handshake must be refused"),
        )
    }

    fn origin_rejection_code(
        response: &tokio_tungstenite::tungstenite::http::Response<Option<Vec<u8>>>,
    ) -> String {
        let body = response.body().as_deref().expect("rejection body");
        let json: serde_json::Value = serde_json::from_slice(body).expect("json body");
        json["error"]["code"]
            .as_str()
            .expect("error code")
            .to_owned()
    }

    const LAN_AUTHORITY: &str = "192.168.1.4:8081";
    const REBINDING_AUTHORITY: &str = "evil.example:8081";
    const ORIGIN_NOT_ALLOWED: &str = "ORIGIN_NOT_ALLOWED";
    const LOOPBACK: &str = "127.0.0.1";
    const EPHEMERAL_PORT: u16 = 0;
    const SHORT_DRAIN_BUDGET: std::time::Duration = std::time::Duration::from_millis(100);
    const PROMISED_BODY_BYTES: usize = 64;
    const INTERIM_RESPONSE_CAPACITY: usize = 256;
    const CONTINUE_STATUS_LINE: &str = "100 Continue";

    #[tokio::test]
    async fn ws_handshake_without_an_origin_header_is_accepted() {
        let (handle, addr) = start_on_an_ephemeral_port(mem_settings(), Vec::new()).await;

        assert!(
            ws_handshake(addr, None).await.is_ok(),
            "a non-browser client sends no Origin and must still connect"
        );

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn ws_handshake_with_a_derived_loopback_origin_is_accepted() {
        let (handle, addr) = start_on_an_ephemeral_port(mem_settings(), Vec::new()).await;
        let port = addr.port();

        for origin in [
            format!("http://127.0.0.1:{port}"),
            format!("https://localhost:{port}"),
            format!("http://[::1]:{port}"),
        ] {
            assert!(
                ws_handshake(addr, Some(&origin)).await.is_ok(),
                "expected accept for {origin}"
            );
        }

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn ws_handshake_with_a_configured_additional_origin_is_accepted() {
        let (handle, addr) = start_on_an_ephemeral_port(
            mem_settings(),
            vec!["https://overlay.example.com".to_owned()],
        )
        .await;

        assert!(
            ws_handshake(addr, Some("https://overlay.example.com"))
                .await
                .is_ok()
        );

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn ws_handshake_with_a_foreign_origin_is_refused_with_the_origin_not_allowed_code() {
        let (handle, addr) = start_on_an_ephemeral_port(mem_settings(), Vec::new()).await;

        let response = refused_ws_handshake(addr, "https://evil.example.com").await;

        assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
        assert_eq!(origin_rejection_code(&response), ORIGIN_NOT_ALLOWED);

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn ws_handshake_from_a_page_served_at_the_requested_lan_address_is_accepted() {
        let (handle, addr) = start_on_an_ephemeral_port(mem_settings(), Vec::new()).await;

        assert!(
            ws_handshake_presenting_host(addr, &format!("http://{LAN_AUTHORITY}"), LAN_AUTHORITY)
                .await
                .is_ok(),
            "a browser source dialling the machine's LAN address must upgrade without configuration"
        );

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn ws_handshake_from_a_self_consistent_name_authority_is_refused_as_rebinding() {
        let (handle, addr) = start_on_an_ephemeral_port(mem_settings(), Vec::new()).await;

        let response = as_http_refusal(
            ws_handshake_presenting_host(
                addr,
                &format!("http://{REBINDING_AUTHORITY}"),
                REBINDING_AUTHORITY,
            )
            .await
            .expect_err("a rebound name must not complete the handshake"),
        );

        assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
        assert_eq!(origin_rejection_code(&response), ORIGIN_NOT_ALLOWED);

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_refused_ws_handshake_never_echoes_the_offending_origin() {
        let (handle, addr) = start_on_an_ephemeral_port(mem_settings(), Vec::new()).await;
        let foreign = "https://evil.example.com";

        let response = refused_ws_handshake(addr, foreign).await;

        for (name, value) in response.headers() {
            let rendered = String::from_utf8_lossy(value.as_bytes());
            assert!(
                !rendered.contains("evil.example.com"),
                "header {name} echoed the rejected origin: {rendered}"
            );
        }
        let body = String::from_utf8_lossy(response.body().as_deref().unwrap_or_default());
        assert!(
            !body.contains("evil.example.com"),
            "rejection body echoed the origin: {body}"
        );

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_refused_ws_handshake_never_registers_a_client() {
        let (handle, addr) = start_on_an_ephemeral_port(mem_settings(), Vec::new()).await;

        refused_ws_handshake(addr, "https://evil.example.com").await;

        assert!(
            handle.snapshot().await.connected_clients.is_empty(),
            "a refused handshake must not reach the socket loop"
        );

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn the_derived_allowlist_uses_the_live_port_not_the_requested_zero() {
        let (handle, addr) = start_on_an_ephemeral_port(mem_settings(), Vec::new()).await;

        assert!(
            ws_handshake(addr, Some(&format!("http://127.0.0.1:{}", addr.port())))
                .await
                .is_ok(),
            "the live port must be on the allowlist"
        );
        let response = refused_ws_handshake(addr, "http://127.0.0.1:0").await;
        assert_eq!(response.status().as_u16(), 403);

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn restart_rebuilds_the_allowlist_from_the_new_bind() {
        let settings = mem_settings();
        let (handle, boot_addr) =
            start_on_an_ephemeral_port(Arc::clone(&settings), Vec::new()).await;
        let new_port = reserve_a_free_port().await;
        crate::config::ServerSettings::save_bind_address(&*settings, "127.0.0.1")
            .await
            .expect("save addr");
        crate::config::ServerSettings::save_port(&*settings, new_port)
            .await
            .expect("save port");

        handle.restart().await.expect("restart");

        let new_addr: std::net::SocketAddr = format!("127.0.0.1:{new_port}").parse().expect("addr");
        assert!(
            ws_handshake(new_addr, Some(&format!("http://127.0.0.1:{new_port}")))
                .await
                .is_ok(),
            "the rebound port must be on the rebuilt allowlist"
        );
        let stale =
            refused_ws_handshake(new_addr, &format!("http://127.0.0.1:{}", boot_addr.port())).await;
        assert_eq!(
            stale.status().as_u16(),
            403,
            "the pre-restart port must not survive on the allowlist"
        );

        handle.stop().await.expect("stop after restart");
    }

    #[tokio::test]
    async fn restart_picks_up_additional_origins_saved_since_boot() {
        let settings = mem_settings();
        let (handle, _boot_addr) =
            start_on_an_ephemeral_port(Arc::clone(&settings), Vec::new()).await;
        let new_port = reserve_a_free_port().await;
        crate::config::ServerSettings::save_bind_address(&*settings, "127.0.0.1")
            .await
            .expect("save addr");
        crate::config::ServerSettings::save_port(&*settings, new_port)
            .await
            .expect("save port");
        crate::config::ServerSettings::save_additional_origins(
            &*settings,
            &["https://dash.test".to_owned()],
        )
        .await
        .expect("save origins");

        handle.restart().await.expect("restart");

        let new_addr: std::net::SocketAddr = format!("127.0.0.1:{new_port}").parse().expect("addr");
        assert!(
            ws_handshake(new_addr, Some("https://dash.test"))
                .await
                .is_ok(),
            "an origin saved since boot must be honoured after a restart"
        );

        handle.stop().await.expect("stop after restart");
    }

    #[tokio::test]
    async fn get_info_without_auth_returns_200_when_reads_not_required() {
        let (handle, addr) = make_server(false, MemCreds::new()).await;
        let url = format!("http://{}/api/v1/info", addr);
        let resp = reqwest::get(&url).await.expect("HTTP request");
        assert_eq!(resp.status().as_u16(), 200);
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn post_without_auth_returns_401() {
        let creds = MemCreds::with_token("fg_secret-token");
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
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn post_with_valid_bearer_passes_auth() {
        let creds = MemCreds::with_token("fg_secret-token");
        let (handle, addr) = make_server(false, creds).await;
        let url = format!("http://{}/api/v1/actions/test-id:do", addr);
        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", "Bearer fg_secret-token")
            .send()
            .await
            .expect("HTTP request");
        assert!(
            resp.status().as_u16() != 401,
            "expected auth to pass, got 401"
        );
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn post_with_wrong_bearer_returns_401() {
        let creds = MemCreds::with_token("fg_secret-token");
        let (handle, addr) = make_server(false, creds).await;
        let url = format!("http://{}/api/v1/actions/test-id:do", addr);
        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", "Bearer wrong-token")
            .send()
            .await
            .expect("HTTP request");
        assert_eq!(resp.status().as_u16(), 401);
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn get_without_auth_returns_401_when_reads_required() {
        let creds = MemCreds::with_token("fg_secret-token");
        let (handle, addr) = make_server(true, creds).await;
        let url = format!("http://{}/api/v1/info", addr);
        let resp = reqwest::get(&url).await.expect("HTTP request");
        assert_eq!(resp.status().as_u16(), 401);
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn get_with_valid_auth_passes_when_reads_required() {
        let creds = MemCreds::with_token("fg_secret-token");
        let (handle, addr) = make_server(true, creds).await;
        let url = format!("http://{}/api/v1/info", addr);
        let resp = reqwest::Client::new()
            .get(&url)
            .header("Authorization", "Bearer fg_secret-token")
            .send()
            .await
            .expect("HTTP request");
        assert_eq!(resp.status().as_u16(), 200);
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn token_regenerate_rejects_old_and_accepts_new() {
        let creds = MemCreds::with_token("fg_old-token");
        let (handle, addr, auth) = make_server_with_shared_auth(false, Arc::clone(&creds)).await;
        let url = format!("http://{}/api/v1/actions/test-id:do", addr);

        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", "Bearer fg_old-token")
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
            .header("Authorization", "Bearer fg_old-token")
            .send()
            .await
            .expect("request");
        assert_eq!(resp.status().as_u16(), 401);

        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {}", new_token.expose()))
            .send()
            .await
            .expect("request");
        assert!(
            resp.status().as_u16() != 401,
            "new token should be accepted after regenerate"
        );

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn stop_is_idempotent_on_repeated_calls() {
        let (handle, _addr) = make_server(false, MemCreds::new()).await;
        handle.stop().await.expect("first stop");
        handle.stop().await.expect("second stop must be a no-op Ok");
    }

    #[tokio::test]
    async fn restart_rebinds_on_persisted_address() {
        let (handle, new_port) = make_server_targeting_a_free_port().await;

        handle.restart().await.expect("restart");

        let url = format!("http://127.0.0.1:{new_port}/api/v1/info");
        let resp = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .expect("HTTP request after restart");
        assert_eq!(resp.status().as_u16(), 200);
        assert_eq!(handle.bind_addr().await.port(), new_port);

        handle.stop().await.expect("stop after restart");
    }

    #[tokio::test]
    async fn kick_client_removes_only_the_named_client_from_the_snapshot() {
        let (handle, state) = make_server_exposing_state().await;
        let (kicked, _kicked_rx) = attach_client(&state, "dashboard-a").await;
        let (_survivor, _survivor_rx) = attach_client(&state, "dashboard-b").await;

        assert!(handle.kick_client(kicked).await);

        let snapshot = handle.snapshot().await;
        let remaining: Vec<&str> = snapshot
            .connected_clients
            .iter()
            .map(|c| c.identification.as_str())
            .collect();
        assert_eq!(remaining, ["dashboard-b"]);

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn kick_client_closes_the_targeted_socket_and_spares_the_others() {
        let (handle, state) = make_server_exposing_state().await;
        let (kicked, mut kicked_rx) = attach_client(&state, "dashboard-a").await;
        let (_survivor, mut survivor_rx) = attach_client(&state, "dashboard-b").await;

        handle.kick_client(kicked).await;

        assert!(matches!(
            kicked_rx.try_recv().expect("close frame"),
            WsFrame::Close
        ));
        assert!(
            survivor_rx.try_recv().is_err(),
            "an untargeted client must keep its socket open"
        );

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn kicking_one_of_two_clients_sharing_an_identification_leaves_the_other_connected() {
        let (handle, state) = make_server_exposing_state().await;
        let (first, mut first_rx) = attach_client(&state, SHARED_IDENTIFICATION).await;
        let (second, mut second_rx) = attach_client(&state, SHARED_IDENTIFICATION).await;

        assert!(handle.kick_client(second).await);

        let snapshot = handle.snapshot().await;
        let remaining: Vec<ClientId> = snapshot.connected_clients.iter().map(|c| c.id).collect();
        assert_eq!(remaining, [first]);
        assert!(matches!(
            second_rx.try_recv().expect("close frame"),
            WsFrame::Close
        ));
        assert!(
            first_rx.try_recv().is_err(),
            "the namesake must keep its socket open"
        );

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn kick_client_with_an_unknown_id_returns_false_and_keeps_every_client() {
        let (handle, state) = make_server_exposing_state().await;
        let (_client, mut rx) = attach_client(&state, "dashboard-a").await;

        assert!(!handle.kick_client(ClientId::next()).await);

        assert_eq!(handle.snapshot().await.connected_clients.len(), 1);
        assert!(
            rx.try_recv().is_err(),
            "a missed lookup must not close anyone"
        );

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn kicking_the_same_client_twice_misses_the_second_time() {
        let (handle, state) = make_server_exposing_state().await;
        let (client, _rx) = attach_client(&state, "dashboard-a").await;

        assert!(handle.kick_client(client).await);
        assert!(!handle.kick_client(client).await);

        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn run_state_reports_running_right_after_bind() {
        let (handle, _addr) = make_server(false, MemCreds::new()).await;
        assert!(*handle.run_state().borrow());
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn stop_flips_run_state_before_the_drain_finishes() {
        let (handle, _addr) = make_server(false, MemCreds::new()).await;
        let mut run_state = handle.run_state();

        let stopper = {
            let handle = handle.clone();
            tokio::spawn(async move { handle.stop().await })
        };

        tokio::time::timeout(HANG_GUARD, run_state.changed())
            .await
            .expect("run state must flip without waiting for the drain")
            .expect("watch sender alive");
        assert!(!*run_state.borrow());

        stopper.await.expect("stop task").expect("stop");
    }

    #[tokio::test]
    async fn run_state_subscribed_after_a_stop_reads_stopped() {
        let (handle, _addr) = make_server(false, MemCreds::new()).await;
        handle.stop().await.expect("stop");
        assert!(
            !*handle.run_state().borrow(),
            "a late subscriber must read the live state, not a fresh channel"
        );
    }

    #[tokio::test]
    async fn restart_returns_run_state_to_running() {
        let (handle, _new_port) = make_server_targeting_a_free_port().await;
        let mut run_state = handle.run_state();

        handle.restart().await.expect("restart");

        assert!(
            *run_state.borrow_and_update(),
            "a restarted server must report running again"
        );

        handle.stop().await.expect("stop after restart");
    }

    struct RecordingListener(Mutex<Vec<forge_storage::OverlayId>>);

    #[async_trait]
    impl forge_runtime::OverlayConnectListener for RecordingListener {
        async fn overlay_connected(&self, identity: &forge_storage::OverlayId) {
            self.0.lock().expect("mutex").push(identity.clone());
        }
    }

    async fn hub_backed_server_on_a_persisted_port(
        hub: &crate::DeliveryHub,
    ) -> (ServerHandle, u16) {
        let settings = mem_settings();
        let port = reserve_a_free_port().await;
        crate::config::ServerSettings::save_bind_address(&*settings, LOOPBACK)
            .await
            .expect("save addr");
        crate::config::ServerSettings::save_port(&*settings, port)
            .await
            .expect("save port");
        let mut config = loopback_config(settings, port, Vec::new());
        let mut overlays = forge_storage::overlay::MockOverlayRepo::new();
        overlays.expect_get_by_credential().returning(|credential| {
            Ok((credential.expose() == OVERLAY_PAGE_CREDENTIAL).then(served_overlay))
        });
        config.overlays = Arc::new(overlays);
        config.delivery_hub = Some(hub.clone());
        (start_server(config).await.expect("start"), port)
    }

    #[tokio::test]
    async fn a_page_registered_on_the_hub_still_receives_content_after_a_server_restart() {
        let hub = crate::DeliveryHub::new(EventBus::new(Arc::new(NullEventLogRepo)));
        let (handle, _port) = hub_backed_server_on_a_persisted_port(&hub).await;
        let identity = forge_storage::OverlayId::new(OVERLAY_IDENTITY);

        handle.restart().await.expect("restart");
        let (client, _initial) = hub
            .adapter()
            .register_client(ClientFilterSet::new(std::collections::HashSet::new()))
            .await;
        let mut page = hub
            .adapter()
            .promote_to_overlay(
                client.id,
                identity.clone(),
                crate::bus_adapter::OverlayClientClass::BrowserSource,
            )
            .await
            .expect("a just-registered client is still on the registry");
        let reached = hub
            .deliver_content(&identity, serde_json::json!({ "value": 3 }), None)
            .await;

        assert_eq!(reached.sources, 1);
        assert!(matches!(page.try_recv(), Ok(WsFrame::Text(_))));
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn the_connect_listener_on_the_hub_hears_an_overlay_page_connecting_after_a_restart() {
        let hub = crate::DeliveryHub::new(EventBus::new(Arc::new(NullEventLogRepo)));
        let listener = Arc::new(RecordingListener(Mutex::new(Vec::new())));
        hub.set_overlay_connect_listener(listener.clone());
        let (handle, port) = hub_backed_server_on_a_persisted_port(&hub).await;
        handle.restart().await.expect("restart");

        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://{LOOPBACK}:{port}/ws/v1/"))
                .await
                .expect("ws connect");
        futures_util::SinkExt::send(
            &mut socket,
            tokio_tungstenite::tungstenite::Message::Text(
                serde_json::json!({
                    "id": "1",
                    "request": "auth",
                    "overlayCredential": OVERLAY_PAGE_CREDENTIAL,
                })
                .to_string()
                .into(),
            ),
        )
        .await
        .expect("the page sends its credential");
        let ack: serde_json::Value =
            serde_json::from_str(&next_text_frame(&mut socket).await).expect("json response");

        assert_eq!(ack["status"], "ok");
        assert_eq!(
            *listener.0.lock().expect("mutex"),
            vec![forge_storage::OverlayId::new(OVERLAY_IDENTITY)]
        );
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_listener_outliving_a_restart_cannot_report_the_new_one_stopped() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let settings = mem_settings();
        let creds = MemCreds::with_token(BEARER_TOKEN);
        let auth = AuthState::load(false, &*creds).await.expect("auth load");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let boot_addr = listener.local_addr().expect("local addr");
        let state = AppState {
            settings: Arc::clone(&settings) as Arc<dyn SettingsRepo>,
            bind_addr: boot_addr,
            allowed_origins: Arc::new(crate::origin::build_allowed_origins(boot_addr, &[])),
            timing: crate::ServerTiming {
                connection_drain_budget: SHORT_DRAIN_BUDGET,
                ..crate::ServerTiming::default()
            },
            ..app_state(auth, creds)
        };
        let (run_state_tx, _run_state_rx) = tokio::sync::watch::channel(true);
        let (reported_tx, reported_rx) = tokio::sync::watch::channel(true);
        let old_listener_reports = reported_tx.clone();
        let (join, shutdown_tx) =
            super::serve_on_with_shutdown(listener, state.clone(), reported_tx);
        let handle = ServerHandle::new(join, shutdown_tx, state, boot_addr, run_state_tx);
        handle.adopt_generation(reported_rx);
        let new_port = reserve_a_free_port().await;
        crate::config::ServerSettings::save_bind_address(&*settings, LOOPBACK)
            .await
            .expect("save addr");
        crate::config::ServerSettings::save_port(&*settings, new_port)
            .await
            .expect("save port");

        let mut lingering = tokio::net::TcpStream::connect(boot_addr)
            .await
            .expect("connect");
        let head = format!(
            "POST /api/v1/globals/score HTTP/1.1\r\nHost: {boot_addr}\r\n\
             Authorization: Bearer {BEARER_TOKEN}\r\nContent-Type: application/json\r\n\
             Content-Length: {PROMISED_BODY_BYTES}\r\nExpect: 100-continue\r\n\r\n"
        );
        lingering.write_all(head.as_bytes()).await.expect("write");
        let mut interim = [0_u8; INTERIM_RESPONSE_CAPACITY];
        let read = tokio::time::timeout(HANG_GUARD, lingering.read(&mut interim))
            .await
            .expect("the handler never asked for the body")
            .expect("read");
        assert!(
            String::from_utf8_lossy(&interim[..read]).contains(CONTINUE_STATUS_LINE),
            "the request never reached a handler waiting on its body"
        );

        tokio::time::timeout(HANG_GUARD, handle.restart())
            .await
            .expect("the restart never finished draining")
            .expect("restart");
        assert!(
            *old_listener_reports.borrow(),
            "the old listener must still be serving the unfinished request after the drain"
        );
        let run_state = handle.run_state();
        assert!(
            *run_state.borrow(),
            "the restarted listener is serving and must report running"
        );

        drop(lingering);
        tokio::time::timeout(HANG_GUARD, old_listener_reports.closed())
            .await
            .expect("the old listener never reported its end");

        assert!(
            *run_state.borrow(),
            "the previous listener finishing reported the live server stopped"
        );
        handle.stop().await.expect("stop after restart");
    }

    #[tokio::test]
    async fn a_stopped_handle_serves_nothing_until_a_restart_brings_it_up() {
        let (handle, port) = stopped_awaiting_a_port(mem_settings()).await;
        let mut run_state = handle.run_state();

        assert!(
            !*run_state.borrow_and_update(),
            "a handle built without a listener must not report running"
        );
        assert!(
            !accepts_on(port).await,
            "nothing may accept on the configured port before the restart"
        );

        handle.restart().await.expect("restart");

        assert!(*run_state.borrow_and_update());
        assert_eq!(handle.bind_addr().await.port(), port);
        assert_eq!(info_status(port).await, reqwest::StatusCode::OK);

        handle.stop().await.expect("stop after restart");
    }

    #[tokio::test]
    async fn stopping_a_handle_that_never_listened_is_an_ok_no_op() {
        let (handle, _port) = stopped_awaiting_a_port(mem_settings()).await;

        handle
            .stop()
            .await
            .expect("stop must not fail with no listener");

        assert!(!*handle.run_state().borrow());
    }

    #[tokio::test]
    async fn a_restart_onto_a_taken_port_reports_bind_and_stays_stopped() {
        let settings = mem_settings();
        let (handle, port) = stopped_awaiting_a_port(Arc::clone(&settings)).await;
        let squatter = TcpListener::bind(format!("{LOOPBACK}:{port}"))
            .await
            .expect("squatter bind");

        let err = handle
            .restart()
            .await
            .expect_err("a port already held must refuse");

        assert!(matches!(err, ServerError::Bind { .. }));
        assert!(
            !*handle.run_state().borrow(),
            "a refused bind must leave the handle stopped, not half started"
        );
        drop(squatter);
    }

    #[tokio::test]
    async fn a_restart_refused_by_the_settings_switch_leaves_the_port_unserved() {
        for (listening, case) in [
            (true, "a server that was serving when the switch went off"),
            (false, "a handle that never listened"),
        ] {
            let settings = mem_settings();
            let (handle, port) = handle_awaiting_a_port(Arc::clone(&settings), listening).await;
            crate::config::ServerSettings::save_enabled(&*settings, false)
                .await
                .expect("save enabled");

            let err = handle
                .restart()
                .await
                .expect_err("a switched-off server must refuse to come up");

            assert!(matches!(err, ServerError::Disabled), "{case}");
            assert!(
                !*handle.run_state().borrow(),
                "{case} must report stopped after the refusal"
            );
            assert!(!accepts_on(port).await, "{case} left a listener behind");
        }
    }

    #[tokio::test]
    async fn a_restart_binds_again_once_the_settings_switch_goes_back_on() {
        let settings = mem_settings();
        let (handle, port) = handle_awaiting_a_port(Arc::clone(&settings), true).await;
        crate::config::ServerSettings::save_enabled(&*settings, false)
            .await
            .expect("save enabled");
        handle
            .restart()
            .await
            .expect_err("the switched-off restart must refuse");

        crate::config::ServerSettings::save_enabled(&*settings, true)
            .await
            .expect("save enabled");
        handle.restart().await.expect("restart once switched on");

        assert!(*handle.run_state().borrow());
        assert_eq!(info_status(port).await, reqwest::StatusCode::OK);

        handle.stop().await.expect("stop after restart");
    }

    #[test]
    fn validate_lan_bind_refuses_every_bind_off_loopback_without_the_flag() {
        for (addr, enabled, refused) in [
            ("0.0.0.0:9595", false, true),
            ("[::]:9595", false, true),
            ("192.168.1.50:9595", false, true),
            ("[::ffff:192.168.1.50]:9595", false, true),
            ("0.0.0.0:9595", true, false),
            ("192.168.1.50:9595", true, false),
            ("127.0.0.1:9595", false, false),
            ("[::1]:9595", false, false),
            ("[::ffff:127.0.0.1]:9595", false, false),
        ] {
            let parsed: std::net::SocketAddr = addr.parse().expect("addr");
            let result = validate_lan_bind(&parsed, enabled);
            assert_eq!(
                matches!(result, Err(ServerError::LanBindNotEnabled { .. })),
                refused,
                "{addr} with the flag {enabled}"
            );
        }
    }

    #[test]
    fn is_lan_bind_tells_loopback_from_every_other_address() {
        for (address, lan) in [
            ("192.168.1.50", true),
            ("::ffff:192.168.1.50", true),
            ("0.0.0.0", true),
            ("::", true),
            ("127.0.0.1", false),
            ("::1", false),
            ("::ffff:127.0.0.1", false),
        ] {
            let parsed: std::net::IpAddr = address.parse().expect("address");
            assert_eq!(is_lan_bind(parsed), lan, "{address}");
        }
    }

    #[tokio::test]
    async fn a_concrete_address_without_the_flag_refuses_to_start() {
        let mut config = loopback_config(mem_settings(), EPHEMERAL_PORT, Vec::new());
        config.bind_addr =
            std::net::SocketAddr::new(LAN_ADDRESS.parse().expect("address"), EPHEMERAL_PORT);

        let result = start_server(config).await;

        assert!(matches!(result, Err(ServerError::LanBindNotEnabled { .. })));
    }

    #[tokio::test]
    async fn only_an_address_off_loopback_demands_a_token_for_reads() {
        for (address, required) in [
            (LAN_ADDRESS, true),
            ("127.0.0.1", false),
            ("::ffff:127.0.0.1", false),
        ] {
            let config = loopback_config(mem_settings(), EPHEMERAL_PORT, Vec::new());
            let bind_addr =
                std::net::SocketAddr::new(address.parse().expect("address"), EPHEMERAL_PORT);

            let state = build_state(config, bind_addr).await.expect("state");

            assert_eq!(state.auth.reads_required(), required, "{address}");
        }
    }

    const OVERLAY_IDENTITY: &str = "goal-box";
    const OVERLAY_PAGE_CREDENTIAL: &str = "2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d";
    const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(30);

    fn served_overlay() -> forge_storage::OverlayDefinition {
        forge_storage::OverlayDefinition {
            id: forge_storage::OverlayId::new(OVERLAY_IDENTITY),
            display_name: "Sub goal".to_owned(),
            kind_id: "overlay.goal".to_owned(),
            enabled: true,
            position: 0,
            config: forge_storage::OverlayConfig::new(),
            config_schema_version: 1,
            generator_version: 0,
            source_overrides: Vec::new(),
            credential: forge_types::OverlayCredential::new(OVERLAY_PAGE_CREDENTIAL),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    async fn make_server_serving_one_overlay()
    -> (ServerHandle, std::net::SocketAddr, Arc<BusAdapter>) {
        let state = state_serving_one_overlay().await;
        let adapter = Arc::clone(&state.bus_adapter);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        (serve_on(listener, state), addr, adapter)
    }

    async fn state_serving_one_overlay() -> AppState {
        let creds = MemCreds::with_token(BEARER_TOKEN);
        let auth = AuthState::load(false, &*creds).await.expect("auth load");
        let creds_dyn: Arc<dyn CredentialsRepo> = creds;
        let mut state = app_state(auth, creds_dyn);

        let mut overlays = forge_storage::overlay::MockOverlayRepo::new();
        overlays.expect_get_by_credential().returning(|credential| {
            Ok((credential.expose() == OVERLAY_PAGE_CREDENTIAL).then(served_overlay))
        });
        state.overlays = Arc::new(overlays);
        state
    }

    async fn next_text_frame<S: ClientIo>(
        socket: &mut tokio_tungstenite::WebSocketStream<S>,
    ) -> String {
        let message = tokio::time::timeout(HANG_GUARD, futures_util::StreamExt::next(socket))
            .await
            .expect("timeout waiting for a frame")
            .expect("the socket closed before a frame arrived")
            .expect("ws error");
        match message {
            tokio_tungstenite::tungstenite::Message::Text(text) => Some(text.to_string()),
            _ => None,
        }
        .expect("the page received a frame that is not text")
    }

    #[tokio::test]
    async fn content_delivered_to_an_overlay_page_arrives_as_a_plain_content_frame() {
        let (handle, addr, adapter) = make_server_serving_one_overlay().await;
        let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws/v1/"))
            .await
            .expect("ws connect");

        futures_util::SinkExt::send(
            &mut socket,
            tokio_tungstenite::tungstenite::Message::Text(
                serde_json::json!({
                    "id": "1",
                    "request": "auth",
                    "overlayCredential": OVERLAY_PAGE_CREDENTIAL,
                })
                .to_string()
                .into(),
            ),
        )
        .await
        .expect("the page sends its credential");
        let ack: serde_json::Value =
            serde_json::from_str(&next_text_frame(&mut socket).await).expect("json response");
        assert_eq!(
            ack["status"], "ok",
            "the page never became an overlay connection, so nothing below is proven"
        );

        adapter
            .deliver_overlay_content(
                &forge_storage::OverlayId::new(OVERLAY_IDENTITY),
                &serde_json::json!({ "label": "Sub goal", "value": 42, "live": true }),
                None,
            )
            .await;

        let delivered: serde_json::Value =
            serde_json::from_str(&next_text_frame(&mut socket).await).expect("json frame");
        assert_eq!(
            delivered,
            serde_json::json!({
                "frame": "content",
                "content": { "label": "Sub goal", "value": 42, "live": true },
            }),
            "the page's runtime reads frame and content exactly as written here"
        );

        handle.stop().await.expect("stop");
    }

    const WILDCARD_EPHEMERAL_BIND: &str = "0.0.0.0:0";
    const OVERLAY_PAGE_BODY: &str = "<p>goal box</p>";
    const OVERLAY_RUNTIME_BODY: &str = "export const runtime = 1;";

    struct LanOverlayServer {
        handle: ServerHandle,
        port: u16,
        hub: crate::DeliveryHub,
        overlay_token: String,
        _overlay_root: tempfile::TempDir,
    }

    impl LanOverlayServer {
        async fn start() -> Self {
            let overlay_root = tempfile::tempdir().expect("tempdir");
            let page = overlay_root.path().join(OVERLAY_IDENTITY);
            let shared = overlay_root.path().join("forge-shared");
            std::fs::create_dir_all(&page).expect("page dir");
            std::fs::create_dir_all(&shared).expect("shared dir");
            std::fs::write(page.join("index.html"), OVERLAY_PAGE_BODY).expect("page");
            std::fs::write(
                page.join("config.json"),
                serde_json::json!({ "credential": OVERLAY_PAGE_CREDENTIAL }).to_string(),
            )
            .expect("config");
            std::fs::write(shared.join("runtime-v1.js"), OVERLAY_RUNTIME_BODY).expect("runtime");

            let mut overlays = forge_storage::overlay::MockOverlayRepo::new();
            overlays
                .expect_get()
                .returning(|id| Ok((id.as_str() == OVERLAY_IDENTITY).then(served_overlay)));
            overlays.expect_get_by_credential().returning(|credential| {
                Ok((credential.expose() == OVERLAY_PAGE_CREDENTIAL).then(served_overlay))
            });
            let hub = crate::DeliveryHub::new(EventBus::new(Arc::new(NullEventLogRepo)));

            let mut config = loopback_config(mem_settings(), EPHEMERAL_PORT, Vec::new());
            config.bind_addr = WILDCARD_EPHEMERAL_BIND.parse().expect("addr");
            config.lan_bind_enabled = true;
            config.overlay_root = overlay_root.path().to_path_buf();
            config.overlays = Arc::new(overlays);
            config.delivery_hub = Some(hub.clone());

            let handle = start_server(config).await.expect("start on all interfaces");
            let port = handle.bind_addr().await.port();
            let overlay_token = handle.overlay_token().await.expose().to_owned();
            Self {
                handle,
                port,
                hub,
                overlay_token,
                _overlay_root: overlay_root,
            }
        }

        fn link(&self, file: &str) -> String {
            format!(
                "http://127.0.0.1:{}/overlays/{}/{file}",
                self.port, self.overlay_token
            )
        }

        async fn lan_get(&self, url: String) -> (u16, String) {
            let response = reqwest::Client::new()
                .get(url)
                .header(reqwest::header::HOST, LAN_AUTHORITY)
                .send()
                .await
                .expect("HTTP request");
            let status = response.status().as_u16();
            (status, response.text().await.expect("body"))
        }

        async fn lan_socket(
            &self,
        ) -> tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        > {
            use tokio_tungstenite::tungstenite::client::IntoClientRequest;

            let mut request = format!("ws://127.0.0.1:{}/ws/v1/", self.port)
                .into_client_request()
                .expect("client request");
            let headers = request.headers_mut();
            headers.insert(
                axum::http::header::HOST,
                LAN_AUTHORITY.parse().expect("host header value"),
            );
            headers.insert(
                axum::http::header::ORIGIN,
                format!("http://{LAN_AUTHORITY}")
                    .parse()
                    .expect("origin header value"),
            );
            tokio_tungstenite::connect_async(request)
                .await
                .expect("the page's websocket must upgrade")
                .0
        }
    }

    async fn request_frame(
        socket: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        frame: serde_json::Value,
    ) -> serde_json::Value {
        futures_util::SinkExt::send(
            socket,
            tokio_tungstenite::tungstenite::Message::Text(frame.to_string().into()),
        )
        .await
        .expect("send frame");
        serde_json::from_str(&next_text_frame(socket).await).expect("json response")
    }

    #[tokio::test]
    async fn an_overlay_link_loads_every_page_file_on_a_lan_bind_without_a_bearer_token() {
        let server = LanOverlayServer::start().await;

        for (file, expected_body) in [
            (format!("{OVERLAY_IDENTITY}/"), OVERLAY_PAGE_BODY.to_owned()),
            (
                format!("{OVERLAY_IDENTITY}/config.json"),
                serde_json::json!({ "credential": OVERLAY_PAGE_CREDENTIAL }).to_string(),
            ),
            (
                "forge-shared/runtime-v1.js".to_owned(),
                OVERLAY_RUNTIME_BODY.to_owned(),
            ),
        ] {
            let (status, body) = server.lan_get(server.link(&file)).await;
            assert_eq!(status, 200, "{file} must load for the browser source");
            assert_eq!(body, expected_body, "{file}");
        }

        server.handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn an_overlay_page_connects_and_receives_content_on_a_lan_bind_without_a_bearer_token() {
        let server = LanOverlayServer::start().await;
        let mut socket = server.lan_socket().await;

        let ack = request_frame(
            &mut socket,
            serde_json::json!({
                "id": "1",
                "request": "auth",
                "overlayCredential": OVERLAY_PAGE_CREDENTIAL,
            }),
        )
        .await;
        assert_eq!(ack["status"], "ok", "the page was refused: {ack}");

        let reached = server
            .hub
            .deliver_content(
                &forge_storage::OverlayId::new(OVERLAY_IDENTITY),
                serde_json::json!({ "value": 42 }),
                None,
            )
            .await;
        assert_eq!(reached.sources, 1);
        let delivered: serde_json::Value =
            serde_json::from_str(&next_text_frame(&mut socket).await).expect("json frame");
        assert_eq!(
            delivered,
            serde_json::json!({ "frame": "content", "content": { "value": 42 } })
        );

        server.handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_lan_bind_refuses_anonymous_api_reads_even_with_the_reads_switch_off() {
        let server = LanOverlayServer::start().await;
        let url = format!("http://127.0.0.1:{}/api/v1/info", server.port);

        let (anonymous, _) = server.lan_get(url.clone()).await;
        let bearer = server.handle.bearer_token().await;
        let authorized = reqwest::Client::new()
            .get(url)
            .bearer_auth(bearer.expose())
            .send()
            .await
            .expect("HTTP request")
            .status();

        assert_eq!(anonymous, 401);
        assert_eq!(authorized, reqwest::StatusCode::OK);
        server.handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_lan_bind_refuses_anonymous_socket_reads_until_the_bearer_is_presented() {
        let server = LanOverlayServer::start().await;
        let mut socket = server.lan_socket().await;
        let info = serde_json::json!({ "id": "1", "request": "getInfo" });

        let refused = request_frame(&mut socket, info.clone()).await;
        let auth = request_frame(
            &mut socket,
            serde_json::json!({
                "id": "2",
                "request": "auth",
                "token": server.handle.bearer_token().await.expose(),
            }),
        )
        .await;
        let allowed = request_frame(&mut socket, info).await;

        assert_eq!(refused["error"]["code"], "UNAUTHENTICATED");
        assert_eq!(auth["status"], "ok");
        assert_eq!(allowed["status"], "ok");
        server.handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_restart_applies_the_lan_read_gate_of_the_address_it_binds() {
        let settings = mem_settings();
        let (handle, port) = handle_awaiting_a_port(Arc::clone(&settings), true).await;
        assert_eq!(info_status(port).await, reqwest::StatusCode::OK);

        crate::config::ServerSettings::save_lan_bind_enabled(&*settings, true)
            .await
            .expect("save flag");
        crate::config::ServerSettings::save_bind_address(&*settings, "0.0.0.0")
            .await
            .expect("save addr");
        handle.restart().await.expect("restart onto the LAN");
        let on_the_lan = info_status(port).await;

        crate::config::ServerSettings::save_bind_address(&*settings, LOOPBACK)
            .await
            .expect("save addr");
        handle.restart().await.expect("restart back to loopback");
        let back_on_loopback = info_status(port).await;

        assert_eq!(on_the_lan, reqwest::StatusCode::UNAUTHORIZED);
        assert_eq!(back_on_loopback, reqwest::StatusCode::OK);
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn concurrent_restarts_both_finish_and_leave_one_serve_task_a_stop_can_end() {
        let (handle, new_port) = make_server_targeting_a_free_port().await;

        let (first, second) = tokio::join!(handle.restart(), handle.restart());

        first.expect("first restart");
        second.expect("second restart");
        assert!(
            accepts_on(new_port).await,
            "a restart must leave a listener"
        );
        handle.stop().await.expect("stop");
        assert!(
            !accepts_on(new_port).await,
            "a serve task survived the stop, so a restart orphaned it"
        );
    }

    #[test]
    fn a_refused_handshake_logs_one_warning_and_withholds_a_malformed_origin() {
        const WELL_FORMED: &str = "https://evil.example.com";

        for (case, origin, logged_verbatim) in [
            ("a bare scheme and authority", WELL_FORMED, true),
            (
                "userinfo in the authority",
                "http://user:pass@evil.example",
                false,
            ),
            (
                "a path past the authority",
                "http://evil.example/overlays/a",
                false,
            ),
            ("a scheme the rule never allows", "ws://evil.example", false),
        ] {
            let (port, lines) = log_capture::capture_blocking(Level::WARN, async move {
                let (handle, addr) = start_on_an_ephemeral_port(mem_settings(), Vec::new()).await;
                refused_ws_handshake(addr, origin).await;
                handle.stop().await.expect("stop");
                addr.port()
            });

            let warnings = log_capture::forge_warnings(&lines);
            assert_eq!(warnings.len(), 1, "{case}: one refusal, one warning");
            let warning = warnings[0];
            assert_eq!(
                warning.field("host"),
                Some(format!("127.0.0.1:{port}").as_str()),
                "{case}: the warning names the authority the page dialled"
            );
            if logged_verbatim {
                assert_eq!(
                    warning.field("origin"),
                    Some(origin),
                    "{case}: a well formed origin is what the user pastes into Settings"
                );
            } else {
                assert!(
                    !lines.iter().any(|line| line.mentions("evil.example")),
                    "{case}: the refusal put attacker-controlled text in the log"
                );
            }
        }
    }

    type ClientSocket<S = tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> =
        tokio_tungstenite::WebSocketStream<S>;

    trait ClientIo: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {}

    impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> ClientIo for S {}

    type DuplexSocket = ClientSocket<tokio::io::DuplexStream>;
    use axum::extract::ws::close_code;
    use tokio_tungstenite::tungstenite::Message as ClientMessage;

    const BEARER_TOKEN: &str = "fg_bearer-under-test";
    const FOREIGN_HOST: &str = "evil.example";
    const LAN_ADDRESS: &str = "192.168.1.50";
    const SHARED_IDENTIFICATION: &str = "obs-browser";
    const AUDIO_OWNER: &str = "alerts";
    const AUDIO_CLIP_BYTES: &[u8] = b"RIFF\0\0\0\0WAVEfmt ";
    const AUDIO_CLIP_DURATION_MS: u64 = 1_200;
    const ADDED_ORIGIN_HOST: &str = "dash.test";
    const PRE_AUTH_LIMIT: usize = 16 * 1024;
    const PROTOCOL_LIMIT: usize = 256 * 1024;
    const PAST_PRE_AUTH_PAYLOAD: usize = 100 * 1024;
    const ROUND_TRIPS: usize = 8;
    const PRE_AUTH_WINDOW: std::time::Duration = std::time::Duration::from_secs(4);
    const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(9);
    const PRE_AUTH_MARGIN: std::time::Duration = std::time::Duration::from_secs(1);
    const SHORT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);
    const SHORT_PRE_AUTH_WINDOW: std::time::Duration = std::time::Duration::from_millis(50);
    const DUPLEX_WS_URL: &str = "ws://127.0.0.1:9515/ws/v1/";

    fn duplex_timing() -> crate::ServerTiming {
        crate::ServerTiming {
            request_read_timeout: READ_TIMEOUT,
            pre_auth_window: PRE_AUTH_WINDOW,
            ..crate::ServerTiming::default()
        }
    }

    async fn serve_over_duplex(mut state: AppState) -> crate::listener::tests::DuplexDialer {
        state.timing = duplex_timing();
        let (listener, dialer) = crate::listener::tests::duplex_listener(READ_TIMEOUT);
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let app = super::build_router(state, shutdown_rx)
            .into_make_service_with_connect_info::<crate::listener::PeerInfo>();
        tokio::spawn(async move {
            let _shutdown_tx = shutdown_tx;
            axum::serve(listener, app).await
        });
        dialer
    }

    async fn duplex_server(
        auth_required_for_reads: bool,
        creds: Arc<MemCreds>,
    ) -> crate::listener::tests::DuplexDialer {
        let auth = AuthState::load(auth_required_for_reads, &*creds)
            .await
            .expect("auth load");
        let creds_dyn: Arc<dyn CredentialsRepo> = creds;
        serve_over_duplex(app_state(auth, creds_dyn)).await
    }

    async fn open_duplex_ws(dialer: &crate::listener::tests::DuplexDialer) -> DuplexSocket {
        tokio_tungstenite::client_async(DUPLEX_WS_URL, dialer.dial())
            .await
            .expect("ws handshake")
            .0
    }

    async fn make_server_with_timing(
        timing: crate::ServerTiming,
    ) -> (ServerHandle, std::net::SocketAddr) {
        let (handle, addr) = start_with_timing(mem_settings(), Vec::new(), timing).await;
        (handle, addr)
    }

    async fn open_ws(addr: std::net::SocketAddr) -> ClientSocket {
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/v1/"))
            .await
            .expect("ws connect")
            .0
    }

    async fn send_text<S: ClientIo>(socket: &mut ClientSocket<S>, text: String) {
        futures_util::SinkExt::send(socket, ClientMessage::Text(text.into()))
            .await
            .expect("send frame");
    }

    async fn next_message<S: ClientIo>(socket: &mut ClientSocket<S>) -> Option<ClientMessage> {
        tokio::time::timeout(HANG_GUARD, futures_util::StreamExt::next(socket))
            .await
            .expect("timeout waiting for the server")
            .and_then(Result::ok)
    }

    async fn ask<S: ClientIo>(
        socket: &mut ClientSocket<S>,
        request: serde_json::Value,
    ) -> serde_json::Value {
        send_text(socket, request.to_string()).await;
        serde_json::from_str(&next_text_frame(socket).await).expect("json response")
    }

    fn close_code_of(message: Option<ClientMessage>) -> Option<u16> {
        match message {
            Some(ClientMessage::Close(Some(frame))) => Some(u16::from(frame.code)),
            _ => None,
        }
    }

    fn get_info_request() -> serde_json::Value {
        serde_json::json!({ "id": "info", "request": "getInfo" })
    }

    fn bearer_auth_request(token: &str) -> serde_json::Value {
        serde_json::json!({ "id": "auth", "request": "auth", "token": token })
    }

    fn code_event_request() -> serde_json::Value {
        serde_json::json!({
            "id": "evt",
            "request": "triggerCodeEvent",
            "name": "qa_probe",
            "args": {},
        })
    }

    async fn authenticated_duplex_ws(
        dialer: &crate::listener::tests::DuplexDialer,
        token: &str,
    ) -> DuplexSocket {
        let mut socket = open_duplex_ws(dialer).await;
        let ack = ask(&mut socket, bearer_auth_request(token)).await;
        assert_eq!(ack["status"], "ok", "the bearer was refused: {ack}");
        socket
    }

    async fn authenticated_ws(addr: std::net::SocketAddr, token: &str) -> ClientSocket {
        let mut socket = open_ws(addr).await;
        let ack = ask(&mut socket, bearer_auth_request(token)).await;
        assert_eq!(ack["status"], "ok", "the bearer was refused: {ack}");
        socket
    }

    async fn assert_still_serving<S: ClientIo>(socket: &mut ClientSocket<S>, who: &str) {
        for _ in 0..ROUND_TRIPS {
            send_text(socket, get_info_request().to_string()).await;
            assert!(
                matches!(next_message(socket).await, Some(ClientMessage::Text(_))),
                "{who} was closed"
            );
        }
    }

    #[tokio::test]
    async fn turning_reads_required_on_closes_an_anonymous_session_with_the_policy_code() {
        let (handle, addr, auth) =
            make_server_with_shared_auth(false, MemCreds::with_token(BEARER_TOKEN)).await;
        let mut anonymous = open_ws(addr).await;
        assert_eq!(
            ask(&mut anonymous, get_info_request()).await["status"],
            "ok"
        );

        auth.set_reads_required(true);

        assert_eq!(
            close_code_of(next_message(&mut anonymous).await),
            Some(close_code::POLICY)
        );
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn once_reads_are_required_a_new_anonymous_reader_is_refused_on_ws_and_http() {
        let (handle, addr, auth) =
            make_server_with_shared_auth(false, MemCreds::with_token(BEARER_TOKEN)).await;

        auth.set_reads_required(true);

        let mut late = open_ws(addr).await;
        let refused = ask(&mut late, get_info_request()).await;
        assert_eq!(refused["error"]["code"], "UNAUTHENTICATED");
        let http = reqwest::get(format!("http://{addr}/api/v1/globals"))
            .await
            .expect("HTTP request");
        assert_eq!(http.status(), reqwest::StatusCode::UNAUTHORIZED);
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn turning_reads_required_on_keeps_bearer_and_overlay_sessions_open() {
        let (handle, addr, _) = make_server_serving_one_overlay().await;
        let mut bearer = authenticated_ws(addr, BEARER_TOKEN).await;
        let mut page = open_ws(addr).await;
        let ack = ask(
            &mut page,
            serde_json::json!({
                "id": "1",
                "request": "auth",
                "overlayCredential": OVERLAY_PAGE_CREDENTIAL,
            }),
        )
        .await;
        assert_eq!(
            ack["status"], "ok",
            "the page never became an overlay session"
        );
        let mut sentinel = open_ws(addr).await;
        assert_eq!(ask(&mut sentinel, get_info_request()).await["status"], "ok");

        handle.auth_state().await.set_reads_required(true);
        assert_eq!(
            close_code_of(next_message(&mut sentinel).await),
            Some(close_code::POLICY),
            "the policy change never reached the sockets, so nothing below is proven"
        );

        assert_still_serving(&mut bearer, "a bearer session").await;
        assert_still_serving(&mut page, "an overlay page").await;
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_restart_applies_a_saved_reads_required_setting() {
        let settings = mem_settings();
        let (handle, port) = handle_awaiting_a_port(Arc::clone(&settings), true).await;
        assert_eq!(info_status(port).await, reqwest::StatusCode::OK);
        crate::config::ServerSettings::save_auth_required_for_reads(&*settings, true)
            .await
            .expect("save reads required");

        handle.restart().await.expect("restart");

        assert_eq!(info_status(port).await, reqwest::StatusCode::UNAUTHORIZED);
        handle.stop().await.expect("stop after restart");
    }

    #[tokio::test]
    async fn regenerating_the_token_closes_a_session_authenticated_with_the_old_one() {
        let creds = MemCreds::with_token(BEARER_TOKEN);
        let (handle, addr, auth) = make_server_with_shared_auth(false, Arc::clone(&creds)).await;
        let mut session = authenticated_ws(addr, BEARER_TOKEN).await;

        auth.regenerate(&*creds).await.expect("regenerate");

        assert_eq!(
            close_code_of(next_message(&mut session).await),
            Some(close_code::POLICY)
        );
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_session_presenting_the_regenerated_token_may_mutate() {
        let creds = MemCreds::with_token(BEARER_TOKEN);
        let (handle, addr, auth) = make_server_with_shared_auth(false, Arc::clone(&creds)).await;
        let fresh = auth.regenerate(&*creds).await.expect("regenerate");

        let mut session = authenticated_ws(addr, fresh.expose()).await;

        assert_eq!(
            ask(&mut session, code_event_request()).await["status"],
            "ok"
        );
        handle.stop().await.expect("stop");
    }

    async fn raw_exchange(addr: std::net::SocketAddr, head: String) -> (u16, String) {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        stream.write_all(head.as_bytes()).await.expect("write");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.expect("read");
        let text = String::from_utf8_lossy(&raw).into_owned();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .expect("status line");
        let body = text
            .split_once("\r\n\r\n")
            .map(|(_, body)| body.to_owned())
            .unwrap_or_default();
        (status, body)
    }

    async fn raw_request_naming(
        addr: std::net::SocketAddr,
        target: &str,
        host: Option<&str>,
    ) -> (u16, String) {
        let head = match host {
            Some(host) => {
                format!("GET {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
            }
            None => format!("GET {target} HTTP/1.0\r\n\r\n"),
        };
        raw_exchange(addr, head).await
    }

    async fn raw_report_naming(addr: std::net::SocketAddr, target: &str, host: &str) -> u16 {
        let body = serde_json::json!({ "verdict": "played" }).to_string();
        let head = format!(
            "POST {target} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        raw_exchange(addr, head).await.0
    }

    fn audio_clip_offer() -> ClipOffer {
        ClipOffer {
            bytes: AUDIO_CLIP_BYTES.to_vec(),
            media_type: ClipMediaType::Wave,
            duration_ms: AUDIO_CLIP_DURATION_MS,
        }
    }

    async fn start_on_all_interfaces() -> (ServerHandle, std::net::SocketAddr) {
        let mut config = loopback_config(mem_settings(), EPHEMERAL_PORT, Vec::new());
        config.bind_addr = std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            EPHEMERAL_PORT,
        );
        config.lan_bind_enabled = true;
        let handle = start_server(config).await.expect("start");
        let addr = handle.bind_addr().await;
        assert!(addr.ip().is_unspecified(), "the bind must stay a wildcard");
        (handle, addr)
    }

    #[tokio::test]
    async fn an_overlay_page_on_another_machine_fetches_its_clip_and_reports_on_an_all_interfaces_bind()
     {
        let (handle, bound) = start_on_all_interfaces().await;
        let dial = std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            bound.port(),
        );
        let page_host = format!("{LAN_ADDRESS}:{}", bound.port());
        let (ticket, outcome) = handle
            .offer_audio_clip(
                &forge_storage::OverlayId::new(AUDIO_OWNER),
                audio_clip_offer(),
            )
            .await
            .expect("offer");

        let (clip_status, clip_body) =
            raw_request_naming(dial, ticket.clip_path(), Some(&page_host)).await;
        let report_status = raw_report_naming(dial, ticket.report_path(), &page_host).await;

        assert_eq!(clip_status, reqwest::StatusCode::OK.as_u16());
        assert_eq!(clip_body.as_bytes(), AUDIO_CLIP_BYTES);
        assert_eq!(report_status, reqwest::StatusCode::NO_CONTENT.as_u16());
        assert_eq!(outcome.recv().await, ClipOutcome::Played);
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn an_all_interfaces_bind_demands_a_token_for_reads_while_the_clip_route_stays_on_its_capability()
     {
        let (handle, bound) = start_on_all_interfaces().await;
        let dial = std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            bound.port(),
        );
        let page_host = format!("{LAN_ADDRESS}:{}", bound.port());
        let (ticket, _outcome) = handle
            .offer_audio_clip(
                &forge_storage::OverlayId::new(AUDIO_OWNER),
                audio_clip_offer(),
            )
            .await
            .expect("offer");

        let (api, _) = raw_request_naming(dial, "/api/v1/info", Some(&page_host)).await;
        let (clip, _) = raw_request_naming(dial, ticket.clip_path(), Some(&page_host)).await;

        assert_eq!(api, reqwest::StatusCode::UNAUTHORIZED.as_u16());
        assert_eq!(clip, reqwest::StatusCode::OK.as_u16());
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_foreign_host_cannot_spend_a_clip_or_settle_its_report() {
        let (handle, addr) = start_on_an_ephemeral_port(mem_settings(), Vec::new()).await;
        let foreign = format!("{FOREIGN_HOST}:{}", addr.port());
        let addressed = addr.to_string();
        let (ticket, outcome) = handle
            .offer_audio_clip(
                &forge_storage::OverlayId::new(AUDIO_OWNER),
                audio_clip_offer(),
            )
            .await
            .expect("offer");

        let (refused_clip, refused_body) =
            raw_request_naming(addr, ticket.clip_path(), Some(&foreign)).await;
        let json: serde_json::Value = serde_json::from_str(&refused_body).expect("json body");
        let (served_clip, served_body) =
            raw_request_naming(addr, ticket.clip_path(), Some(&addressed)).await;
        let refused_report = raw_report_naming(addr, ticket.report_path(), &foreign).await;
        let accepted_report = raw_report_naming(addr, ticket.report_path(), &addressed).await;

        let misdirected = reqwest::StatusCode::MISDIRECTED_REQUEST.as_u16();
        assert_eq!(refused_clip, misdirected);
        assert_eq!(json["error"]["code"], "HOST_NOT_ALLOWED");
        assert_eq!(served_clip, reqwest::StatusCode::OK.as_u16());
        assert_eq!(served_body.as_bytes(), AUDIO_CLIP_BYTES);
        assert_eq!(refused_report, misdirected);
        assert_eq!(accepted_report, reqwest::StatusCode::NO_CONTENT.as_u16());
        assert_eq!(outcome.recv().await, ClipOutcome::Played);
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_request_naming_a_foreign_host_is_misdirected_on_every_host_guarded_route() {
        let (handle, addr) = start_on_an_ephemeral_port(mem_settings(), Vec::new()).await;
        let foreign = format!("{FOREIGN_HOST}:{}", addr.port());

        for target in [
            "/api/v1/info",
            "/overlays/alerts/config.json",
            crate::routes::overlays::FAVICON_PATH,
        ] {
            let (status, body) = raw_request_naming(addr, target, Some(&foreign)).await;
            assert_eq!(
                status,
                reqwest::StatusCode::MISDIRECTED_REQUEST.as_u16(),
                "{target}"
            );
            let json: serde_json::Value = serde_json::from_str(&body).expect("json body");
            assert_eq!(json["error"]["code"], "HOST_NOT_ALLOWED", "{target}");
        }
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn requests_naming_an_address_localhost_an_allowed_origin_or_no_host_are_served() {
        let (handle, addr) = start_on_an_ephemeral_port(
            mem_settings(),
            vec![format!("https://{ADDED_ORIGIN_HOST}")],
        )
        .await;
        let port = addr.port();

        for host in [
            Some(format!("127.0.0.1:{port}")),
            Some(format!("[::1]:{port}")),
            Some(format!("localhost:{port}")),
            Some(format!("{ADDED_ORIGIN_HOST}:{port}")),
            None,
        ] {
            let (status, _) = raw_request_naming(addr, "/api/v1/info", host.as_deref()).await;
            assert_eq!(status, reqwest::StatusCode::OK.as_u16(), "host {host:?}");
        }
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn an_anonymous_frame_past_the_pre_auth_limit_closes_with_the_size_code() {
        let (handle, addr) = make_server(false, MemCreds::with_token(BEARER_TOKEN)).await;
        let mut anonymous = open_ws(addr).await;

        send_text(&mut anonymous, "x".repeat(PRE_AUTH_LIMIT)).await;
        assert!(
            matches!(
                next_message(&mut anonymous).await,
                Some(ClientMessage::Text(_))
            ),
            "a frame exactly at the limit must still be answered"
        );

        send_text(&mut anonymous, "x".repeat(PRE_AUTH_LIMIT + 1)).await;
        assert_eq!(
            close_code_of(next_message(&mut anonymous).await),
            Some(close_code::SIZE)
        );
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn an_authenticated_session_may_send_frames_past_the_pre_auth_limit() {
        let (handle, addr) = make_server(false, MemCreds::with_token(BEARER_TOKEN)).await;
        let mut session = authenticated_ws(addr, BEARER_TOKEN).await;

        send_text(&mut session, "x".repeat(PAST_PRE_AUTH_PAYLOAD)).await;

        assert!(matches!(
            next_message(&mut session).await,
            Some(ClientMessage::Text(_))
        ));
        handle.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn a_frame_past_the_protocol_limit_is_refused_before_the_socket_loop_sees_it() {
        let (handle, addr) = make_server(false, MemCreds::with_token(BEARER_TOKEN)).await;
        let mut session = authenticated_ws(addr, BEARER_TOKEN).await;

        futures_util::SinkExt::send(
            &mut session,
            ClientMessage::Binary(vec![0_u8; PROTOCOL_LIMIT + 1].into()),
        )
        .await
        .expect("send frame");
        let ending = next_message(&mut session).await;

        assert!(
            !matches!(ending, Some(ClientMessage::Text(_))),
            "the oversized frame was answered"
        );
        assert_ne!(
            close_code_of(ending),
            Some(close_code::UNSUPPORTED),
            "the oversized frame reached the socket loop"
        );
        handle.stop().await.expect("stop");
    }

    #[tokio::test(start_paused = true)]
    async fn a_connection_that_never_sends_a_request_is_closed_exactly_at_the_read_deadline() {
        use tokio::io::AsyncReadExt as _;

        let dialer = duplex_server(false, MemCreds::new()).await;
        let mut silent = dialer.dial();
        let opened = tokio::time::Instant::now();

        let mut buf = [0_u8; 1];
        let outcome = silent.read(&mut buf).await;

        assert!(
            matches!(outcome, Ok(0) | Err(_)),
            "the server answered nothing yet sent bytes"
        );
        assert_eq!(opened.elapsed(), READ_TIMEOUT);
    }

    #[tokio::test]
    async fn the_configured_read_timeout_reaches_the_listener() {
        use tokio::io::AsyncReadExt as _;

        let (handle, addr) = make_server_with_timing(crate::ServerTiming {
            request_read_timeout: SHORT_READ_TIMEOUT,
            ..crate::ServerTiming::default()
        })
        .await;
        let mut silent = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let opened = tokio::time::Instant::now();

        let mut buf = [0_u8; 1];
        let outcome = tokio::time::timeout(HANG_GUARD, silent.read(&mut buf))
            .await
            .expect("the silent connection outlived the configured read deadline");

        assert!(
            matches!(outcome, Ok(0) | Err(_)),
            "the server answered nothing yet sent bytes"
        );
        assert!(opened.elapsed() >= SHORT_READ_TIMEOUT);
        handle.stop().await.expect("stop");
    }

    #[tokio::test(start_paused = true)]
    async fn an_upgraded_websocket_outlives_the_read_deadline() {
        let dialer = duplex_server(false, MemCreds::new()).await;
        let mut socket = open_duplex_ws(&dialer).await;
        assert_eq!(ask(&mut socket, get_info_request()).await["status"], "ok");

        tokio::time::sleep(READ_TIMEOUT + PRE_AUTH_MARGIN).await;

        assert_eq!(ask(&mut socket, get_info_request()).await["status"], "ok");
    }

    #[tokio::test(start_paused = true)]
    async fn a_silent_socket_is_closed_with_policy_exactly_at_the_pre_auth_window() {
        let dialer = duplex_server(false, MemCreds::new()).await;
        let mut socket = open_duplex_ws(&dialer).await;
        let opened = tokio::time::Instant::now();

        let closed = futures_util::StreamExt::next(&mut socket).await;

        assert_eq!(
            close_code_of(closed.and_then(Result::ok)),
            Some(close_code::POLICY),
            "a socket that never spoke must be closed once the window passes"
        );
        assert_eq!(opened.elapsed(), PRE_AUTH_WINDOW);
    }

    #[tokio::test]
    async fn the_configured_pre_auth_window_reaches_the_socket() {
        let (handle, addr) = make_server_with_timing(crate::ServerTiming {
            pre_auth_window: SHORT_PRE_AUTH_WINDOW,
            ..crate::ServerTiming::default()
        })
        .await;
        let mut socket = open_ws(addr).await;

        let closed = tokio::time::timeout(HANG_GUARD, futures_util::StreamExt::next(&mut socket))
            .await
            .expect("the silent socket outlived the configured window");

        assert_eq!(
            close_code_of(closed.and_then(Result::ok)),
            Some(close_code::POLICY)
        );
        handle.stop().await.expect("stop");
    }

    #[tokio::test(start_paused = true)]
    async fn a_socket_that_spoke_once_stays_open_past_the_window_when_reads_are_open() {
        let dialer = duplex_server(false, MemCreds::new()).await;
        let mut socket = open_duplex_ws(&dialer).await;

        assert_eq!(ask(&mut socket, get_info_request()).await["status"], "ok");
        tokio::time::sleep(PRE_AUTH_WINDOW + PRE_AUTH_MARGIN).await;

        assert_still_serving(&mut socket, "a reader that had spoken while reads are open").await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_unauthenticated_socket_that_spoke_is_still_closed_at_the_window_when_reads_are_required()
     {
        let dialer = duplex_server(true, MemCreds::with_token(BEARER_TOKEN)).await;
        let mut socket = open_duplex_ws(&dialer).await;

        assert_eq!(
            ask(&mut socket, get_info_request()).await["error"]["code"],
            "UNAUTHENTICATED",
            "an unauthenticated request must be answered, not silently dropped"
        );
        tokio::time::sleep(PRE_AUTH_WINDOW + PRE_AUTH_MARGIN).await;

        assert_eq!(
            close_code_of(next_message(&mut socket).await),
            Some(close_code::POLICY),
            "having sent a message must not exempt an unauthenticated socket at the window"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_bearer_authenticated_socket_stays_open_past_the_window_when_reads_are_required() {
        let dialer = duplex_server(true, MemCreds::with_token(BEARER_TOKEN)).await;
        let mut session = authenticated_duplex_ws(&dialer, BEARER_TOKEN).await;

        tokio::time::sleep(PRE_AUTH_WINDOW + PRE_AUTH_MARGIN).await;

        assert_still_serving(&mut session, "a bearer-authenticated session").await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_overlay_session_stays_open_past_the_window_when_reads_are_required() {
        let state = state_serving_one_overlay().await;
        state.auth.set_reads_required(true);
        let dialer = serve_over_duplex(state).await;
        let mut page = open_duplex_ws(&dialer).await;
        let ack = ask(
            &mut page,
            serde_json::json!({
                "id": "1",
                "request": "auth",
                "overlayCredential": OVERLAY_PAGE_CREDENTIAL,
            }),
        )
        .await;
        assert_eq!(
            ack["status"], "ok",
            "the page never became an overlay session"
        );

        tokio::time::sleep(PRE_AUTH_WINDOW + PRE_AUTH_MARGIN).await;

        assert_still_serving(&mut page, "an overlay session").await;
    }

    #[tokio::test]
    async fn a_socket_that_joins_after_shutdown_is_signalled_is_closed_at_once() {
        let creds = MemCreds::new();
        let auth = AuthState::load(false, &*creds).await.expect("auth load");
        let state = app_state(auth, creds as Arc<dyn CredentialsRepo>);
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(true);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let app = super::build_router(state, shutdown_rx)
            .into_make_service_with_connect_info::<crate::listener::PeerInfo>();
        let serving = tokio::spawn(async move {
            axum::serve(crate::listener::GuardedListener::new(listener), app).await
        });
        let mut late = open_ws(addr).await;

        let first = next_message(&mut late).await;

        assert!(
            matches!(first, Some(ClientMessage::Close(_))),
            "a socket admitted after shutdown was left open: {first:?}"
        );
        serving.abort();
    }

    #[tokio::test]
    async fn stop_and_restart_close_every_connected_socket() {
        for (lifecycle, restart) in [("stop", false), ("restart", true)] {
            let (handle, _new_port) = make_server_targeting_a_free_port().await;
            let boot_addr = handle.bind_addr().await;
            let mut first = open_ws(boot_addr).await;
            let mut second = open_ws(boot_addr).await;

            if restart {
                handle.restart().await.expect("restart");
            } else {
                handle.stop().await.expect("stop");
            }

            for (who, socket) in [("first", &mut first), ("second", &mut second)] {
                assert!(
                    matches!(next_message(socket).await, Some(ClientMessage::Close(_))),
                    "{lifecycle} left the {who} socket open"
                );
            }
            handle.stop().await.expect("stop");
        }
    }

    #[tokio::test]
    async fn a_started_server_gates_tokenless_overlay_files_unless_the_stored_setting_opts_out() {
        let root = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(root.path().join("page.html"), b"<p>page</p>")
            .await
            .expect("write page");

        for (stored, tokenless_status, case) in [
            (None, 404, "setting never written"),
            (Some(true), 404, "setting on"),
            (Some(false), 200, "setting off"),
        ] {
            let settings = mem_settings();
            if let Some(required) = stored {
                crate::config::ServerSettings::save_http_overlay_require_token(
                    &*settings, required,
                )
                .await
                .expect("save setting");
            }
            let mut config = loopback_config(settings, EPHEMERAL_PORT, Vec::new());
            config.overlay_root = root.path().to_path_buf();
            let handle = start_server(config).await.expect("start");
            let addr = handle.bind_addr().await;

            let status = reqwest::get(format!("http://{addr}/overlays/page.html"))
                .await
                .expect("request")
                .status()
                .as_u16();

            assert_eq!(status, tokenless_status, "{case}");
            handle.stop().await.expect("stop");
        }
    }
}
