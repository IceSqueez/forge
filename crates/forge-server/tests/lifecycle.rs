#![expect(clippy::unwrap_used, reason = "test fixtures")]

use std::net::SocketAddr;
use std::sync::Arc;

use forge_server::test_support::ServerDeps;
use forge_server::{ServerConfig, ServerHandle, ServerSettings, start_server, stopped_server};
use forge_storage::CredentialsRepo;
use forge_storage::credentials::MockCredentialsRepo;
use forge_storage::test_support::{MemoryCredentials, MemorySettings};
use reqwest::StatusCode;

const LOOPBACK: &str = "127.0.0.1";
const OVERLAY_ROOT_NAME: &str = "overlays";

static ONE_SERVER_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn config(
    settings: Arc<MemorySettings>,
    credentials: Arc<dyn CredentialsRepo>,
    overlay_root: &std::path::Path,
) -> ServerConfig {
    ServerDeps::new(overlay_root)
        .with_settings(settings)
        .with_credentials(credentials)
        .build()
}

struct Server {
    handle: ServerHandle,
    addr: SocketAddr,
    _root: tempfile::TempDir,
}

impl Server {
    async fn running() -> Self {
        Self::running_with(Arc::new(MemoryCredentials::default())).await
    }

    async fn running_with(credentials: Arc<dyn CredentialsRepo>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let overlays = root.path().join(OVERLAY_ROOT_NAME);
        std::fs::create_dir(&overlays).unwrap();
        std::fs::write(overlays.join("page.html"), "<p>live</p>").unwrap();
        let handle = start_server(config(
            Arc::new(MemorySettings::new()),
            credentials,
            &overlays,
        ))
        .await
        .unwrap();
        let addr = handle.bind_addr().await;
        Self {
            handle,
            addr,
            _root: root,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    async fn info_status(&self) -> StatusCode {
        reqwest::get(self.url("/api/v1/info"))
            .await
            .unwrap()
            .status()
    }

    async fn run_action(&self, bearer: Option<&str>) -> StatusCode {
        let request = reqwest::Client::new().post(self.url("/api/v1/actions/any-id:do"));
        let request = match bearer {
            Some(token) => request.bearer_auth(token),
            None => request,
        };
        request.send().await.unwrap().status()
    }
}

#[tokio::test]
async fn a_started_server_binds_a_real_port_serves_reads_and_reports_itself_running() {
    let _released_port_stays_ours = ONE_SERVER_AT_A_TIME.lock().await;
    let server = Server::running().await;

    assert_ne!(server.addr.port(), 0);
    assert_eq!(server.info_status().await, StatusCode::OK);
    assert!(server.handle.run_state().is_running());

    server.handle.stop().await.unwrap();
}

#[tokio::test]
async fn a_stopped_server_refuses_connections_and_reports_itself_stopped() {
    let _released_port_stays_ours = ONE_SERVER_AT_A_TIME.lock().await;
    let server = Server::running().await;

    server.handle.stop().await.unwrap();

    assert!(tokio::net::TcpStream::connect(server.addr).await.is_err());
    assert!(!server.handle.run_state().is_running());
}

#[tokio::test]
async fn a_restart_applies_the_saved_settings_and_serves_again() {
    let _released_port_stays_ours = ONE_SERVER_AT_A_TIME.lock().await;
    let settings = Arc::new(MemorySettings::new());
    let root = tempfile::tempdir().unwrap();
    let handle = start_server(config(
        Arc::clone(&settings),
        Arc::new(MemoryCredentials::default()),
        root.path(),
    ))
    .await
    .unwrap();
    let addr = handle.bind_addr().await;
    ServerSettings::save_bind_address(settings.as_ref(), LOOPBACK)
        .await
        .unwrap();
    ServerSettings::save_port(settings.as_ref(), addr.port())
        .await
        .unwrap();

    handle.restart().await.unwrap();

    let status = reqwest::get(format!("http://{addr}/api/v1/info"))
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::OK);
    assert!(handle.run_state().is_running());
    handle.stop().await.unwrap();
}

#[tokio::test]
async fn a_server_built_stopped_stays_closed_until_the_user_starts_it() {
    let _released_port_stays_ours = ONE_SERVER_AT_A_TIME.lock().await;
    let root = tempfile::tempdir().unwrap();
    let handle = stopped_server(config(
        Arc::new(MemorySettings::new()),
        Arc::new(MemoryCredentials::default()),
        root.path(),
    ))
    .await
    .unwrap();

    assert!(!handle.run_state().is_running());
}

#[tokio::test]
async fn a_mutating_request_needs_the_current_bearer_and_a_rotation_retires_the_old_one() {
    let _released_port_stays_ours = ONE_SERVER_AT_A_TIME.lock().await;
    let server = Server::running().await;
    let original = server.handle.bearer_token().await.expose().to_owned();
    assert_eq!(server.run_action(None).await, StatusCode::UNAUTHORIZED);
    assert_ne!(
        server.run_action(Some(&original)).await,
        StatusCode::UNAUTHORIZED
    );

    let rotated = server.handle.regenerate_bearer_token().await.unwrap();

    assert_eq!(
        server.run_action(Some(&original)).await,
        StatusCode::UNAUTHORIZED
    );
    assert_ne!(
        server.run_action(Some(rotated.expose())).await,
        StatusCode::UNAUTHORIZED
    );
    server.handle.stop().await.unwrap();
}

#[tokio::test]
async fn reads_become_bearer_only_once_the_handle_requires_it() {
    let _released_port_stays_ours = ONE_SERVER_AT_A_TIME.lock().await;
    let server = Server::running().await;

    server.handle.set_reads_require_auth(true).await.unwrap();

    assert_eq!(server.info_status().await, StatusCode::UNAUTHORIZED);
    server.handle.stop().await.unwrap();
}

#[tokio::test]
async fn a_credential_store_that_cannot_keep_the_token_fails_the_start() {
    let _released_port_stays_ours = ONE_SERVER_AT_A_TIME.lock().await;
    let mut credentials = MockCredentialsRepo::new();
    credentials.expect_load().returning(|_| Ok(None));
    credentials
        .expect_store()
        .returning(|_, _| Err(forge_storage::StorageError::Encryption));
    let root = tempfile::tempdir().unwrap();

    let outcome = start_server(config(
        Arc::new(MemorySettings::new()),
        Arc::new(credentials),
        root.path(),
    ))
    .await;

    assert!(outcome.is_err());
}
