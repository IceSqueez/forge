#![expect(clippy::panic, reason = "test fixtures fail loudly by design")]

use std::future::Future;
use std::net::TcpListener;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forge_audio::{AudioError, AudioSink, AudioSinkFactory};
use forge_platform_core::{EndpointSurface, IntegrationDeclaration, PlatformEndpoints};
use forge_storage::{DataProvider, SettingsRepo, set_integration_enabled, write_setting};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{OutputDevice, Secret, Shared};
use tokio::runtime::Builder;

use super::RecordingSink;
use crate::host::{HostInputs, build_host};
use crate::host_handles::Host;
use crate::platform_identities::{OAuthClientIdentity, PlatformIdentities};

const HANG_GUARD: Duration = Duration::from_secs(60);
const RUNTIME_SHUTDOWN_GRACE: Duration = Duration::from_secs(1);
const CHILD_MARKER: &str = "FORGE_BOOT_SMOKE_CHILD";

pub const ALL_INTEGRATIONS: [IntegrationDeclaration; 10] = [
    forge_platform_twitch::TWITCH_INTEGRATION,
    forge_platform_youtube::YOUTUBE_INTEGRATION,
    forge_platform_kick::KICK_INTEGRATION,
    forge_obs::OBS_INTEGRATION,
    forge_vtube::VTUBE_INTEGRATION,
    forge_discord::DISCORD_INTEGRATION,
    forge_midi::MIDI_INTEGRATION,
    forge_hotkey::HOTKEY_INTEGRATION,
    forge_donatello::DONATELLO_INTEGRATION,
    forge_monobank::MONOBANK_INTEGRATION,
];

fn endpoint_override(surface: EndpointSurface, port: u16) -> String {
    let scheme = if surface.env_var().contains("_WS_") {
        "ws"
    } else {
        "http"
    };
    format!("{scheme}://127.0.0.1:{port}")
}

fn assert_nothing_dialled(listener: &TcpListener) {
    listener.set_nonblocking(true).unwrap();
    let error = listener.accept().map(|_| ()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
}

pub fn runs_here_in_isolation(test_path: &str) -> bool {
    if std::env::var_os(CHILD_MARKER).is_some() {
        return true;
    }
    let home = tempfile::tempdir().unwrap();
    let data_dir = home.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let network_sentinel = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = network_sentinel.local_addr().unwrap().port();

    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args([test_path, "--exact", "--test-threads=1", "--nocapture"])
        .env(CHILD_MARKER, "1")
        .env("FORGE_DATA_DIR", &data_dir)
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", home.path())
        .env_remove("FORGE_CREDENTIAL_KEY_FILE");
    for surface in crate::integration_catalog::endpoint_surfaces() {
        child.env(surface.env_var(), endpoint_override(surface, port));
    }
    let absent_bus = format!("unix:path={}", home.path().join("absent-bus").display());
    child
        .env("DBUS_SESSION_BUS_ADDRESS", &absent_bus)
        .env("DBUS_SYSTEM_BUS_ADDRESS", &absent_bus);

    let output = child.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "isolated run of {test_path} failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_nothing_dialled(&network_sentinel);
    false
}

pub fn on_app_runtime(body: impl Future<Output = ()>) {
    let runtime = Builder::new_multi_thread().enable_all().build().unwrap();
    let outcome = catch_unwind(AssertUnwindSafe(|| runtime.block_on(body)));
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN_GRACE);
    if let Err(panic) = outcome {
        resume_unwind(panic);
    }
}

pub async fn guarded<F: Future>(work: &str, future: F) -> F::Output {
    tokio::time::timeout(HANG_GUARD, future)
        .await
        .unwrap_or_else(|_| panic!("{work} hung"))
}

#[derive(Default)]
pub struct RecordingSinkFactory {
    builds: AtomicUsize,
    pub sink: Arc<RecordingSink>,
}

impl RecordingSinkFactory {
    pub fn builds(&self) -> usize {
        self.builds.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl AudioSinkFactory for RecordingSinkFactory {
    async fn build(&self, _: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
        self.builds.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::clone(&self.sink) as Arc<dyn AudioSink>)
    }
}

pub fn fake_identities() -> PlatformIdentities {
    let identity = |name: &str| OAuthClientIdentity {
        client_id: format!("{name}-client"),
        client_secret: Secret::new(format!("{name}-secret")),
    };
    PlatformIdentities {
        twitch_client_id: Some("twitch-client".to_owned()),
        youtube: Some(identity("youtube")),
        kick: Some(identity("kick")),
    }
}

fn database_url() -> String {
    let path = forge_paths::data_dir().join(forge_storage::DATABASE_FILE_NAME);
    format!("sqlite://{}?mode=rwc", path.display())
}

pub async fn open_store() -> Arc<dyn DataProvider> {
    Arc::new(SqliteBackend::open(&database_url()).await.unwrap())
}

pub async fn seed_integrations_and_server_off(settings: &dyn SettingsRepo) {
    for integration in &ALL_INTEGRATIONS {
        set_integration_enabled(settings, &integration.id, false)
            .await
            .unwrap();
    }
    write_setting(settings, &forge_server::settings::ENABLED, &false)
        .await
        .unwrap();
}

pub struct BootedHost {
    pub host: Host,
    pub sinks: Arc<RecordingSinkFactory>,
}

pub async fn boot_host(
    provider: Arc<dyn DataProvider>,
    platform_identities: PlatformIdentities,
    anonymous_donor: Shared<String>,
) -> BootedHost {
    let sinks = Arc::new(RecordingSinkFactory::default());
    let inputs = HostInputs {
        provider,
        data_dir: forge_paths::data_dir(),
        endpoints: PlatformEndpoints::from_env(&crate::integration_catalog::endpoint_surfaces())
            .unwrap(),
        hotkey_main_thread: forge_hotkey::testing::offline_main_thread_link(),
        audio_sinks: Arc::clone(&sinks) as Arc<dyn AudioSinkFactory>,
        local_tts_engines: Vec::new(),
        platform_identities,
        anonymous_donor,
    };
    let host = guarded("the boot", build_host(inputs)).await;
    BootedHost { host, sinks }
}
