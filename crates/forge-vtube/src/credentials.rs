use std::sync::Arc;

use forge_events::EventPublisher;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::Secret;
use serde::{Deserialize, Serialize};

use crate::vts::endpoint::{DEFAULT_VTS_HOST, DEFAULT_VTS_PORT, endpoint_url_with};
use crate::{VTubeClient, VTubeConfig};

pub const VTUBE_CREDENTIAL_ID: &str = "vtube:default";

fn default_host() -> String {
    DEFAULT_VTS_HOST.to_owned()
}

fn default_port() -> u16 {
    DEFAULT_VTS_PORT
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VTubeCredentials {
    #[serde(serialize_with = "Secret::serialize_exposed")]
    pub token: Secret,
    pub api_version: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub secure: bool,
}

pub async fn store(
    creds: &dyn CredentialsRepo,
    token: &str,
    api_version: &str,
    host: &str,
    port: u16,
    secure: bool,
) -> Result<(), StorageError> {
    let bundle = serde_json::json!({
        "token": token,
        "api_version": api_version,
        "host": host,
        "port": port,
        "secure": secure,
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
        endpoint: endpoint_url_with(&stored.host, stored.port, stored.secure),
    };
    Ok(Arc::new(VTubeClient::connect(cfg, publisher, creds_arc)))
}

pub async fn clear(creds: &dyn CredentialsRepo) -> Result<bool, StorageError> {
    creds.delete(&CredentialId::new(VTUBE_CREDENTIAL_ID)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tests::{MockCreds, MockPublisher};

    #[test]
    fn a_bundle_saved_without_an_endpoint_loads_against_the_vts_loopback_defaults() {
        let stored = r#"{"token":"tok-abc123","api_version":"1.0"}"#;

        let creds: VTubeCredentials = serde_json::from_str(stored).unwrap();

        assert_eq!(creds.token.expose(), "tok-abc123");
        assert_eq!(creds.host, "127.0.0.1");
        assert_eq!(creds.port, 8001);
    }

    #[tokio::test]
    async fn a_bundle_saved_before_the_secure_flag_existed_dials_plain_websocket() {
        let repo = MockCreds::new();
        repo.insert(
            VTUBE_CREDENTIAL_ID,
            r#"{"token":"tok","api_version":"1.0","host":"203.0.113.7","port":9123}"#,
        );
        let publisher = MockPublisher::new();

        let client = load_and_connect(&*repo, publisher.publisher(), repo.creds())
            .await
            .unwrap();

        assert_eq!(client.config.endpoint, "ws://203.0.113.7:9123/");
    }

    #[tokio::test]
    async fn a_custom_endpoint_survives_a_store_then_load_cycle() {
        let repo = MockCreds::new();

        store(&*repo, "tok-xyz", "1.0", "192.168.1.50", 9123, false)
            .await
            .unwrap();
        let back = load(&*repo).await.unwrap().unwrap();

        assert_eq!(back.token.expose(), "tok-xyz");
        assert_eq!(back.host, "192.168.1.50");
        assert_eq!(back.port, 9123);
    }

    #[tokio::test]
    async fn the_secure_flag_survives_a_store_then_load_cycle_in_both_states() {
        for secure in [true, false] {
            let repo = MockCreds::new();

            store(&*repo, "tok", "1.0", "192.0.2.50", 9123, secure)
                .await
                .unwrap();

            assert_eq!(load(&*repo).await.unwrap().unwrap().secure, secure);
        }
    }

    #[tokio::test]
    async fn a_stored_secure_endpoint_is_redialed_over_tls() {
        let repo = MockCreds::new();
        store(&*repo, "tok", "1.0", "203.0.113.7", 9123, true)
            .await
            .unwrap();
        let publisher = MockPublisher::new();

        let client = load_and_connect(&*repo, publisher.publisher(), repo.creds())
            .await
            .unwrap();

        assert_eq!(client.config.endpoint, "wss://203.0.113.7:9123/");
    }

    #[test]
    fn debug_redacts_token() {
        let creds = VTubeCredentials {
            token: Secret::new("super-secret-vtube-token"),
            api_version: "1.0".into(),
            host: "127.0.0.1".into(),
            port: 8001,
            secure: false,
        };

        let s = format!("{creds:?}");

        assert!(!s.contains("super-secret-vtube-token"), "Debug leaked: {s}");
    }

    #[tokio::test]
    async fn load_and_connect_dials_the_stored_endpoint_not_the_compiled_in_default() {
        let repo = MockCreds::new();
        store(&*repo, "tok", "1.0", "203.0.113.7", 9123, false)
            .await
            .unwrap();
        let publisher = MockPublisher::new();

        let client = load_and_connect(&*repo, publisher.publisher(), repo.creds())
            .await
            .unwrap();

        assert_eq!(client.config.endpoint, "ws://203.0.113.7:9123/");
    }

    const SAVED_BUNDLE: &str = r#"{"token":"tok-abc123","api_version":"1.0","host":"192.0.2.50","port":9123,"secure":false}"#;

    #[test]
    fn a_saved_bundle_saves_back_byte_identical() {
        let loaded: VTubeCredentials = serde_json::from_str(SAVED_BUNDLE).unwrap();

        assert_eq!(serde_json::to_string(&loaded).unwrap(), SAVED_BUNDLE);
    }

    #[tokio::test]
    async fn store_writes_the_document_earlier_builds_saved() {
        let repo = MockCreds::new();

        store(&*repo, "tok-abc123", "1.0", "192.0.2.50", 9123, false)
            .await
            .unwrap();

        let written = repo.value(VTUBE_CREDENTIAL_ID).expect("a stored bundle");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&written).unwrap(),
            serde_json::from_str::<serde_json::Value>(SAVED_BUNDLE).unwrap()
        );
    }
}
