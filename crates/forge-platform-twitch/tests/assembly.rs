#![allow(clippy::unwrap_used)]
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_platform_core::PlatformEndpoints;
use forge_platform_twitch::credentials::{StoredCredential, store_credential};
use forge_platform_twitch::{TWITCH_CREDENTIAL_ID, TwitchAssembly};
use forge_registry::SubActionRegistry;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::OAuthToken;
use time::OffsetDateTime;

#[derive(Default)]
struct CountingRepo {
    rows: Mutex<BTreeMap<String, String>>,
    streamer_loads: AtomicUsize,
    load_fails: AtomicBool,
}

#[async_trait]
impl CredentialsRepo for CountingRepo {
    async fn store(&self, id: &CredentialId, plaintext_bundle: &str) -> Result<(), StorageError> {
        self.rows
            .lock()
            .unwrap()
            .insert(id.to_string(), plaintext_bundle.to_owned());
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        if id.to_string() == TWITCH_CREDENTIAL_ID {
            self.streamer_loads.fetch_add(1, Ordering::SeqCst);
            if self.load_fails.load(Ordering::SeqCst) {
                return Err(StorageError::Decryption);
            }
        }
        Ok(self.rows.lock().unwrap().get(&id.to_string()).cloned())
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self.rows.lock().unwrap().remove(&id.to_string()).is_some())
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .keys()
            .map(|id| CredentialId::new(id.clone()))
            .collect())
    }

    async fn last_refresh(&self, _: &CredentialId) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

fn assembly_over(repo: &Arc<CountingRepo>) -> TwitchAssembly {
    let bus: Arc<dyn EventPublisher> = Arc::new(forge_events::test_support::NullPublisher);
    TwitchAssembly::new(
        &mut SubActionRegistry::new(),
        Arc::clone(repo) as Arc<dyn CredentialsRepo>,
        &bus,
        &PlatformEndpoints::default(),
        "assembly-test-client".to_owned(),
    )
    .unwrap()
}

async fn store_streamer(repo: &CountingRepo, login: &str) {
    store_credential(
        repo,
        &StoredCredential {
            access_token: OAuthToken::new("access"),
            refresh_token: Some(OAuthToken::new("refresh")),
            user_id: "4242".to_owned(),
            login: login.to_owned(),
            expires_at: None,
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn opening_a_session_reads_the_stored_identity_exactly_once() {
    let repo = Arc::new(CountingRepo::default());
    store_streamer(&repo, "streamer").await;
    let assembly = assembly_over(&repo);
    let before = repo.streamer_loads.load(Ordering::SeqCst);

    let session = assembly.open_session().await.unwrap();

    assert_eq!(repo.streamer_loads.load(Ordering::SeqCst) - before, 1);
    assert!(session.into_bundle().is_some());
}

#[tokio::test]
async fn a_session_without_stored_credentials_yields_no_bundle() {
    let repo = Arc::new(CountingRepo::default());
    let assembly = assembly_over(&repo);

    let session = assembly.open_session().await.unwrap();

    assert!(session.into_bundle().is_none());
}

#[tokio::test]
async fn an_unreadable_credential_store_fails_the_session_instead_of_reading_as_signed_out() {
    let repo = Arc::new(CountingRepo::default());
    let assembly = assembly_over(&repo);
    repo.load_fails.store(true, Ordering::SeqCst);

    assert!(assembly.open_session().await.is_err());
}

#[tokio::test]
async fn a_stored_identity_without_a_login_still_yields_a_bundle() {
    let repo = Arc::new(CountingRepo::default());
    store_streamer(&repo, "").await;
    let assembly = assembly_over(&repo);

    let session = assembly.open_session().await.unwrap();

    assert!(session.into_bundle().is_some());
}

#[tokio::test]
async fn the_assembly_reports_configured_only_once_credentials_are_stored() {
    let repo = Arc::new(CountingRepo::default());
    let assembly = assembly_over(&repo);
    let before = assembly.is_configured().await.unwrap();

    store_streamer(&repo, "streamer").await;
    let after = assembly.is_configured().await.unwrap();

    assert_eq!((before, after), (false, true));
}
