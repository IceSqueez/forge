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

    #[test]
    fn credentials_serde_roundtrip() {
        let creds = VTubeCredentials {
            token: "tok-abc123".into(),
            api_version: "1.0".into(),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: VTubeCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.token, creds.token);
        assert_eq!(back.api_version, creds.api_version);
    }

    #[test]
    fn debug_redacts_token() {
        let creds = VTubeCredentials {
            token: "super-secret-vtube-token".into(),
            api_version: "1.0".into(),
        };
        let s = format!("{creds:?}");
        assert!(!s.contains("super-secret-vtube-token"));
        assert!(s.contains("***"));
        assert!(s.contains("1.0"));
    }
}
