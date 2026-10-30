use std::sync::Arc;

use forge_events::EventPublisher;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::Secret;

use crate::{ObsClient, ObsError};

pub const OBS_CREDENTIAL_ID: &str = "obs:default";

#[derive(Debug, Clone)]
pub struct StoredCredential {
    pub url: String,
    pub password: Secret,
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
    let password = Secret::new(bundle["password"].as_str().unwrap_or(""));
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
    let pw = Some(cred.password.expose()).filter(|password| !password.is_empty());
    let client = ObsClient::connect(&cred.url, pw, bus).await?;
    Ok(Arc::new(client))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use time::OffsetDateTime;

    use super::*;

    const SAVED_BUNDLE: &str = r#"{"url":"ws://192.0.2.10:4455","password":"obs-pass-sentinel"}"#;

    #[derive(Default)]
    struct SingleSlot(Mutex<Option<String>>);

    #[async_trait]
    impl CredentialsRepo for SingleSlot {
        async fn store(&self, _: &CredentialId, bundle: &str) -> Result<(), StorageError> {
            *self.0.lock().unwrap() = Some(bundle.to_owned());
            Ok(())
        }

        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.0.lock().unwrap().clone())
        }

        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.0.lock().unwrap().take().is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(Vec::new())
        }

        async fn last_refresh(
            &self,
            _: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_saved_bundle_loads_its_url_and_password() {
        let repo = SingleSlot(Mutex::new(Some(SAVED_BUNDLE.to_owned())));

        let loaded = load(&repo).await.unwrap().expect("a saved credential");

        assert_eq!(
            (loaded.url.as_str(), loaded.password.expose()),
            ("ws://192.0.2.10:4455", "obs-pass-sentinel")
        );
    }

    #[tokio::test]
    async fn store_writes_the_document_earlier_builds_saved() {
        let repo = SingleSlot::default();

        store(&repo, "192.0.2.10", 4455, "obs-pass-sentinel")
            .await
            .unwrap();

        let written = repo.0.lock().unwrap().clone().expect("a stored bundle");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&written).unwrap(),
            serde_json::from_str::<serde_json::Value>(SAVED_BUNDLE).unwrap()
        );
    }

    #[test]
    fn debug_output_of_a_stored_credential_never_carries_the_password() {
        let password = "obs-debug-secret-4f3e2d";
        let credential = StoredCredential {
            url: "ws://192.0.2.10:4455".to_owned(),
            password: Secret::new(password),
        };

        let rendered = format!("{credential:?} {credential:#?}");

        assert!(!rendered.contains(password), "leaked: {rendered}");
    }
}
