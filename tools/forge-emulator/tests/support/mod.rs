#![allow(dead_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use forge_emulator::harness::launch::{ForgeCommand, GameGuard, HyprlandProbe, LivePaths};
use forge_events::{Event, EventPublisher};
use forge_platform_core::{PlatformEndpoints, PlatformError};
use futures_util::future::BoxFuture;
use tokio::time::timeout;

pub const EMULATOR: &str = env!("CARGO_BIN_EXE_forge-emulator");

pub const BIND_FAILURE_LINE: &str = "2026-10-28T10:00:00.000000Z ERROR server failed to start, leaving it off signal=server_bind outcome=failed cause=address_unavailable addr=127.0.0.1:40000 port=40000 error=could not bind to 127.0.0.1:40000: Address already in use (os error 98)";

const GONE_POLL: Duration = Duration::from_millis(10);

pub type TokenSource =
    Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct QuietPublisher;

impl EventPublisher for QuietPublisher {
    fn publish(&self, _: Event) {}
}

pub fn endpoints_from(overrides: &[(&'static str, String)]) -> PlatformEndpoints {
    PlatformEndpoints::resolve(|variable| {
        overrides
            .iter()
            .find(|(name, _)| *name == variable)
            .map(|(_, url)| OsString::from(url))
    })
    .unwrap()
}

pub fn static_token_source(token: String) -> TokenSource {
    Arc::new(move || {
        let token = token.clone();
        Box::pin(async move { Ok(token) })
    })
}

pub fn process_is_gone(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(stat) => stat
            .rsplit_once(") ")
            .is_some_and(|(_, rest)| rest.starts_with('Z')),
    }
}

pub async fn wait_until_gone(pid: u32, deadline: Duration) -> bool {
    timeout(deadline, async {
        while !process_is_gone(pid) {
            tokio::time::sleep(GONE_POLL).await;
        }
    })
    .await
    .is_ok()
}

pub fn stub_forge(script: &Path) -> ForgeCommand {
    ForgeCommand {
        program: PathBuf::from("/bin/sh"),
        args: vec![script.as_os_str().to_owned()],
    }
}

pub fn clear_desktop() -> GameGuard {
    GameGuard::probing(HyprlandProbe::command(
        "/bin/sh",
        vec!["-c".into(), "printf '[]'".into(), "sh".into()],
    ))
}

pub fn elsewhere() -> LivePaths {
    LivePaths {
        data_dir: PathBuf::from("/nonexistent-live-user/.local/share/forge"),
        home: PathBuf::from("/nonexistent-live-user"),
    }
}

pub fn shipped_json(directory: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> =
        std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join(directory))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
    files.sort();
    files
}
