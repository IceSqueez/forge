#![allow(dead_code, clippy::unwrap_used)]
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_platform_core::{EndpointSurface, PlatformEndpoints, PlatformError};
use forge_platform_youtube::YoutubeBroadcaster;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use futures::future::BoxFuture;
use time::OffsetDateTime;
use wiremock::MockServer;

pub(crate) const DATA_API_PREFIX: &str = "/youtube/v3";

pub(crate) type TokenSource =
    Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;
pub(crate) type BroadcasterSource =
    Arc<dyn Fn() -> BoxFuture<'static, Result<YoutubeBroadcaster, PlatformError>> + Send + Sync>;

pub(crate) struct SilentBus;

impl EventPublisher for SilentBus {
    fn publish(&self, _event: Event) {}
}

#[derive(Default)]
pub(crate) struct MemoryRepo {
    rows: Mutex<BTreeMap<String, String>>,
}

#[async_trait]
impl CredentialsRepo for MemoryRepo {
    async fn store(&self, id: &CredentialId, plaintext_bundle: &str) -> Result<(), StorageError> {
        self.rows
            .lock()
            .unwrap()
            .insert(id.to_string(), plaintext_bundle.to_owned());
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(self.rows.lock().unwrap().get(&id.to_string()).cloned())
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self.rows.lock().unwrap().remove(&id.to_string()).is_some())
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(Vec::new())
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

pub(crate) fn endpoints(overrides: &[(EndpointSurface, String)]) -> PlatformEndpoints {
    let table: BTreeMap<&'static str, String> = overrides
        .iter()
        .map(|(surface, url)| (surface.env_var(), url.clone()))
        .collect();
    PlatformEndpoints::resolve(|variable| table.get(variable).map(OsString::from)).unwrap()
}

pub(crate) fn data_api_override(server: &MockServer) -> PlatformEndpoints {
    endpoints(&[(
        EndpointSurface::YouTubeDataApi,
        format!("{}{DATA_API_PREFIX}", server.uri()),
    )])
}

pub(crate) fn fixed_token(token: &'static str) -> TokenSource {
    Arc::new(move || Box::pin(async move { Ok(token.to_owned()) }))
}

pub(crate) fn fixed_broadcaster(
    channel_id: &'static str,
    channel_title: &'static str,
) -> BroadcasterSource {
    Arc::new(move || {
        Box::pin(async move {
            Ok(YoutubeBroadcaster {
                channel_id: channel_id.to_owned(),
                channel_title: channel_title.to_owned(),
            })
        })
    })
}
