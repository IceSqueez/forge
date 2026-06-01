#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_app::cloud_tts_boot::register_cloud_engines;
use forge_storage::CredentialsRepo;
use forge_storage::{CredentialId, StorageError};
use forge_tts_cloud::credentials::{
    AZURE_CREDENTIAL_ID, ELEVENLABS_CREDENTIAL_ID, OPENAI_CREDENTIAL_ID, POLLY_CREDENTIAL_ID,
};
use forge_tts_core::{EngineId, TtsRegistry};
use time::OffsetDateTime;

struct MemCreds(Mutex<HashMap<String, String>>);

impl MemCreds {
    fn empty() -> Arc<Self> {
        Arc::new(Self(Mutex::new(HashMap::new())))
    }

    fn with(key: &str, value: &str) -> Arc<Self> {
        let mut map = HashMap::new();
        map.insert(key.to_owned(), value.to_owned());
        Arc::new(Self(Mutex::new(map)))
    }
}

#[async_trait]
impl CredentialsRepo for MemCreds {
    async fn store(&self, id: &CredentialId, bundle: &str) -> Result<(), StorageError> {
        self.0
            .lock()
            .unwrap()
            .insert(id.as_str().to_owned(), bundle.to_owned());
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
            .map(|k| CredentialId::new(k.clone()))
            .collect())
    }

    async fn last_refresh(
        &self,
        _id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

#[tokio::test]
async fn no_credentials_leaves_registry_empty() {
    let registry = std::sync::RwLock::new(TtsRegistry::new());
    let creds = MemCreds::empty();
    register_cloud_engines(&registry, creds.as_ref()).await;
    assert!(registry.read().unwrap().engine_ids().is_empty());
}

#[tokio::test]
async fn azure_credentials_register_azure_engine() {
    let registry = std::sync::RwLock::new(TtsRegistry::new());
    let json = r#"{"api_key":"k","region":"eastus"}"#;
    let creds = MemCreds::with(AZURE_CREDENTIAL_ID, json);
    register_cloud_engines(&registry, creds.as_ref()).await;
    assert!(
        registry
            .read()
            .unwrap()
            .get(&EngineId("azure".into()))
            .is_some()
    );
}

#[tokio::test]
async fn elevenlabs_credentials_register_elevenlabs_engine() {
    let registry = std::sync::RwLock::new(TtsRegistry::new());
    let json = r#"{"api_key":"xi-key"}"#;
    let creds = MemCreds::with(ELEVENLABS_CREDENTIAL_ID, json);
    register_cloud_engines(&registry, creds.as_ref()).await;
    assert!(
        registry
            .read()
            .unwrap()
            .get(&EngineId("elevenlabs".into()))
            .is_some()
    );
}

#[tokio::test]
async fn openai_credentials_register_openai_engine() {
    let registry = std::sync::RwLock::new(TtsRegistry::new());
    let json = r#"{"api_key":"sk-test","base_url":null}"#;
    let creds = MemCreds::with(OPENAI_CREDENTIAL_ID, json);
    register_cloud_engines(&registry, creds.as_ref()).await;
    assert!(
        registry
            .read()
            .unwrap()
            .get(&EngineId("openai".into()))
            .is_some()
    );
}

#[tokio::test]
async fn polly_credentials_register_polly_engine() {
    let registry = std::sync::RwLock::new(TtsRegistry::new());
    let json = r#"{"access_key_id":"AKID","secret_access_key":"s","region":"us-east-1"}"#;
    let creds = MemCreds::with(POLLY_CREDENTIAL_ID, json);
    register_cloud_engines(&registry, creds.as_ref()).await;
    assert!(
        registry
            .read()
            .unwrap()
            .get(&EngineId("polly".into()))
            .is_some()
    );
}

#[tokio::test]
async fn malformed_json_skips_engine() {
    let registry = std::sync::RwLock::new(TtsRegistry::new());
    let creds = MemCreds::with(AZURE_CREDENTIAL_ID, "not-json{{{");
    register_cloud_engines(&registry, creds.as_ref()).await;
    assert!(
        registry
            .read()
            .unwrap()
            .get(&EngineId("azure".into()))
            .is_none()
    );
}

#[tokio::test]
async fn all_four_engines_registered_when_all_credentials_present() {
    let registry = std::sync::RwLock::new(TtsRegistry::new());
    let mut map = HashMap::new();
    map.insert(
        AZURE_CREDENTIAL_ID.to_owned(),
        r#"{"api_key":"k","region":"eastus"}"#.to_owned(),
    );
    map.insert(
        ELEVENLABS_CREDENTIAL_ID.to_owned(),
        r#"{"api_key":"xi"}"#.to_owned(),
    );
    map.insert(
        OPENAI_CREDENTIAL_ID.to_owned(),
        r#"{"api_key":"sk","base_url":null}"#.to_owned(),
    );
    map.insert(
        POLLY_CREDENTIAL_ID.to_owned(),
        r#"{"access_key_id":"A","secret_access_key":"s","region":"us-east-1"}"#.to_owned(),
    );
    let creds = Arc::new(MemCreds(Mutex::new(map)));
    register_cloud_engines(&registry, creds.as_ref()).await;
    let ids = registry.read().unwrap().engine_ids();
    assert_eq!(ids.len(), 4);
}
