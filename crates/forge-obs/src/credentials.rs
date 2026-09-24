use std::sync::Arc;

use forge_events::EventPublisher;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};

use crate::{ObsClient, ObsError};

pub const OBS_CREDENTIAL_ID: &str = "obs:default";

#[derive(Clone)]
pub struct StoredCredential {
    pub url: String,
    pub password: String,
}

impl std::fmt::Debug for StoredCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredCredential")
            .field("url", &self.url)
            .field("password", &"<redacted>")
            .finish()
    }
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

pub async fn clear(creds: &dyn CredentialsRepo) -> Result<bool, StorageError> {
    creds.delete(&CredentialId::new(OBS_CREDENTIAL_ID)).await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_of_a_stored_credential_never_carries_the_password() {
        let password = "obs-debug-secret-4f3e2d";
        let credential = StoredCredential {
            url: "ws://192.0.2.10:4455".to_owned(),
            password: password.to_owned(),
        };

        let rendered = format!("{credential:?} {credential:#?}");

        assert!(!rendered.contains(password), "leaked: {rendered}");
    }
}
