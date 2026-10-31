#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_emulator::harness::launch::{ForgeCommand, GameGuard, HyprlandProbe, LivePaths};
use forge_events::{Event, EventPublisher};
use forge_platform_core::{PlatformEndpoints, PlatformError};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use futures_util::future::BoxFuture;
use time::OffsetDateTime;
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

#[derive(Default)]
pub struct MemoryCredentials(Mutex<HashMap<String, String>>);

impl MemoryCredentials {
    pub fn holding(id: &str, plaintext: &str) -> Self {
        Self(Mutex::new(HashMap::from([(
            id.to_owned(),
            plaintext.to_owned(),
        )])))
    }
}

#[async_trait]
impl CredentialsRepo for MemoryCredentials {
    async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
        self.0
            .lock()
            .unwrap()
            .insert(id.as_str().to_owned(), plaintext.to_owned());
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(self.0.lock().unwrap().get(id.as_str()).cloned())
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self.0.lock().unwrap().remove(id.as_str()).is_some())
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .keys()
            .map(|key| CredentialId::new(key.clone()))
            .collect())
    }

    async fn last_refresh(&self, _: &CredentialId) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
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
