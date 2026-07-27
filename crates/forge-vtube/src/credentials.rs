use std::sync::Arc;

use forge_events::EventPublisher;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use serde::{Deserialize, Serialize};

use crate::client::{DEFAULT_VTS_HOST, DEFAULT_VTS_PORT};
use crate::{VTubeClient, VTubeConfig};

pub const VTUBE_CREDENTIAL_ID: &str = "vtube:default";

fn default_host() -> String {
    DEFAULT_VTS_HOST.to_owned()
}

fn default_port() -> u16 {
    DEFAULT_VTS_PORT
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VTubeCredentials {
    pub token: String,
    pub api_version: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl std::fmt::Debug for VTubeCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VTubeCredentials")
            .field("token", &"***")
            .field("api_version", &self.api_version)
            .field("host", &self.host)
            .field("port", &self.port)
            .finish()
    }
}

pub async fn store(
    creds: &dyn CredentialsRepo,
    token: &str,
    api_version: &str,
    host: &str,
    port: u16,
) -> Result<(), StorageError> {
    let bundle = serde_json::json!({
        "token": token,
        "api_version": api_version,
        "host": host,
        "port": port,
    });
    creds
        .store(&CredentialId::new(VTUBE_CREDENTIAL_ID), &bundle.to_string())
        .await
}

pub async fn load(creds: &dyn CredentialsRepo) -> Result<Option<VTubeCredentials>, StorageError> {
    let Some(json) = creds.load(&CredentialId::new(VTUBE_CREDENTIAL_ID)).await? else {
        return Ok(None);
    };
    let parsed: VTubeCredentials = serde_json::from_str(&json)?;
    Ok(Some(parsed))
}

#[derive(Debug, thiserror::Error)]
pub enum VTubeConnectError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("VTube Studio credentials not stored")]
    NotStored,
}

pub async fn load_and_connect(
    creds: &dyn CredentialsRepo,
    publisher: Arc<dyn EventPublisher>,
    creds_arc: Arc<dyn CredentialsRepo>,
) -> Result<Arc<VTubeClient>, VTubeConnectError> {
    let stored = load(creds).await?.ok_or(VTubeConnectError::NotStored)?;
    let cfg = VTubeConfig {
        endpoint: format!("ws://{}:{}/", stored.host, stored.port),
    };
    Ok(Arc::new(VTubeClient::connect(cfg, publisher, creds_arc)))
}

pub async fn clear(creds: &dyn CredentialsRepo) -> Result<bool, StorageError> {
    creds.delete(&CredentialId::new(VTUBE_CREDENTIAL_ID)).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::client::tests::{MockCreds, MockPublisher};

    // Why: bundles persisted before the endpoint became configurable carry only token and
    // api_version. Without the serde defaults every one of those bundles fails to parse, and
    // the user is pushed back through the VTube Studio approval popup on upgrade.
    #[test]
    fn a_bundle_saved_without_an_endpoint_loads_against_the_vts_loopback_defaults() {
        let stored = r#"{"token":"tok-abc123","api_version":"1.0"}"#;

        let creds: VTubeCredentials = serde_json::from_str(stored).unwrap();

        assert_eq!(creds.token, "tok-abc123");
        assert_eq!(creds.host, "127.0.0.1");
        assert_eq!(creds.port, 8001);
    }

    #[tokio::test]
    async fn a_custom_endpoint_survives_a_store_then_load_cycle() {
        let repo = MockCreds::new();

        store(&*repo, "tok-xyz", "1.0", "192.168.1.50", 9123)
            .await
            .unwrap();
        let back = load(&*repo).await.unwrap().unwrap();

        assert_eq!(back.token, "tok-xyz");
        assert_eq!(back.host, "192.168.1.50");
        assert_eq!(back.port, 9123);
    }

    #[test]
    fn debug_redacts_token() {
        let creds = VTubeCredentials {
            token: "super-secret-vtube-token".into(),
            api_version: "1.0".into(),
            host: "127.0.0.1".into(),
            port: 8001,
        };

        let s = format!("{creds:?}");

        assert!(!s.contains("super-secret-vtube-token"), "Debug leaked: {s}");
        assert!(s.contains("***"));
    }

    // Why: the loader used to dial the compiled-in default endpoint no matter what was stored,
    // so a user running VTube Studio on another port reconnected to the wrong socket on every
    // restart. TEST-NET-3 keeps the spawned supervisor's dial off any real host.
    #[tokio::test]
    async fn load_and_connect_dials_the_stored_endpoint_not_the_compiled_in_default() {
        let repo = MockCreds::new();
        store(&*repo, "tok", "1.0", "203.0.113.7", 9123)
            .await
            .unwrap();
        let publisher = MockPublisher::new();

        let client = load_and_connect(&*repo, publisher.publisher(), repo.creds())
            .await
            .unwrap();

        assert_eq!(client.config.endpoint, "ws://203.0.113.7:9123/");
    }
}
