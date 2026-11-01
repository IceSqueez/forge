use std::sync::Arc;

use forge_events::EventPublisher;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use serde::{Deserialize, Serialize};

use crate::{ObsClient, ObsEndpoint, ObsError};

pub const OBS_CREDENTIAL_ID: &str = "obs:default";

#[derive(Serialize, Deserialize)]
struct StoredBundle {
    url: String,
    #[serde(default)]
    password: String,
}

pub async fn store(
    creds: &dyn CredentialsRepo,
    endpoint: &ObsEndpoint,
) -> Result<(), StorageError> {
    let bundle = StoredBundle {
        url: endpoint.url(),
        password: endpoint.password().unwrap_or_default().to_owned(),
    };
    creds
        .store(
            &CredentialId::new(OBS_CREDENTIAL_ID),
            &serde_json::to_string(&bundle)?,
        )
        .await
}

pub async fn load(creds: &dyn CredentialsRepo) -> Result<Option<ObsEndpoint>, StorageError> {
    let Some(json) = creds.load(&CredentialId::new(OBS_CREDENTIAL_ID)).await? else {
        return Ok(None);
    };
    let bundle: StoredBundle = serde_json::from_str(&json)?;
    ObsEndpoint::parse(&bundle.url, &bundle.password)
        .map(Some)
        .map_err(|error| StorageError::Parse(format!("unusable OBS credential: {error}")))
}

pub async fn clear(creds: &dyn CredentialsRepo) -> Result<bool, StorageError> {
    creds.delete(&CredentialId::new(OBS_CREDENTIAL_ID)).await
}

pub async fn load_and_connect(
    creds: &dyn CredentialsRepo,
    bus: Arc<dyn EventPublisher>,
) -> Result<Arc<ObsClient>, ObsError> {
    let endpoint = load(creds).await?.ok_or(ObsError::NotStored)?;
    let client = ObsClient::connect(&endpoint, bus).await?;
    Ok(Arc::new(client))
}

#[cfg(test)]
mod tests {
    use forge_storage::test_support::MemoryCredentials;

    use super::*;

    const SAVED_BUNDLE: &str = r#"{"url":"ws://192.0.2.10:4455","password":"obs-pass-sentinel"}"#;

    #[tokio::test]
    async fn a_saved_bundle_loads_its_url_and_password() {
        let repo = MemoryCredentials::holding(OBS_CREDENTIAL_ID, SAVED_BUNDLE);

        let loaded = load(&repo).await.unwrap().expect("a saved credential");

        assert_eq!(
            (loaded.url().as_str(), loaded.password()),
            ("ws://192.0.2.10:4455", Some("obs-pass-sentinel"))
        );
    }

    #[tokio::test]
    async fn store_writes_the_document_earlier_builds_saved() {
        let repo = MemoryCredentials::default();

        store(
            &repo,
            &ObsEndpoint::new("192.0.2.10", 4455, "obs-pass-sentinel"),
        )
        .await
        .unwrap();

        let written = repo.stored(OBS_CREDENTIAL_ID).expect("a stored bundle");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&written).unwrap(),
            serde_json::from_str::<serde_json::Value>(SAVED_BUNDLE).unwrap()
        );
    }

    #[test]
    fn debug_output_of_a_stored_credential_never_carries_the_password() {
        let password = "obs-debug-secret-4f3e2d";
        let credential = ObsEndpoint::new("192.0.2.10", 4455, password);

        let rendered = format!("{credential:?} {credential:#?}");

        assert!(!rendered.contains(password), "leaked: {rendered}");
    }
}
