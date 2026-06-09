use std::sync::Arc;

use forge_events::EventPublisher;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};

use crate::{ObsClient, ObsError};

pub const OBS_CREDENTIAL_ID: &str = "obs:default";

#[derive(Debug, Clone)]
pub struct StoredCredential {
    pub url: String,
    pub password: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ObsConnectError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("OBS credentials not stored")]
    NotStored,
    #[error(transparent)]
    Connect(#[from] ObsError),
}

pub async fn store(
    creds: &dyn CredentialsRepo,
    host: &str,
    port: u16,
    password: &str,
) -> Result<(), StorageError> {
    let bundle = serde_json::json!({
        "url": format!("ws://{host}:{port}"),
        "password": password,
    });
    creds
        .store(&CredentialId::new(OBS_CREDENTIAL_ID), &bundle.to_string())
        .await
}

pub async fn load(creds: &dyn CredentialsRepo) -> Result<Option<StoredCredential>, StorageError> {
    let Some(json) = creds.load(&CredentialId::new(OBS_CREDENTIAL_ID)).await? else {
        return Ok(None);
    };
    let bundle: serde_json::Value = serde_json::from_str(&json)?;
    let url = bundle["url"]
        .as_str()
        .ok_or_else(|| StorageError::Parse("missing url in OBS credential".into()))?
        .to_owned();
    let password = bundle["password"].as_str().unwrap_or("").to_owned();
    Ok(Some(StoredCredential { url, password }))
}

pub async fn load_and_connect(
    creds: &dyn CredentialsRepo,
    bus: Arc<dyn EventPublisher>,
) -> Result<Arc<ObsClient>, ObsConnectError> {
    let cred = load(creds).await?.ok_or(ObsConnectError::NotStored)?;
    let pw: Option<&str> = if cred.password.is_empty() {
        None
    } else {
        Some(&cred.password)
    };
    let client = ObsClient::connect(&cred.url, pw, bus).await?;
    Ok(Arc::new(client))
}

pub async fn store_and_connect(
    creds: &dyn CredentialsRepo,
    bus: Arc<dyn EventPublisher>,
    host: &str,
    port: u16,
    password: &str,
) -> Result<Arc<ObsClient>, ObsConnectError> {
    store(creds, host, port, password).await?;
    let pw: Option<&str> = if password.is_empty() {
        None
    } else {
        Some(password)
    };
    let client = ObsClient::connect(&format!("ws://{host}:{port}"), pw, bus).await?;
    Ok(Arc::new(client))
}
