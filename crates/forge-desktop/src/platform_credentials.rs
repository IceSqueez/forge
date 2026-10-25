use std::sync::Arc;

use forge_platform_core::{PlatformError, credential_storage_error};
use forge_storage::{CredentialId, CredentialsRepo};
use forge_types::PlatformId;

#[derive(Clone, Default)]
pub struct PlatformCredentialManagers {
    pub twitch: Option<Arc<forge_platform_twitch::TwitchCredentialsManager>>,
    pub youtube: Option<Arc<forge_platform_youtube::YoutubeCredentialsManager>>,
    pub kick: Option<Arc<forge_platform_kick::KickCredentialsManager>>,
}

impl PlatformCredentialManagers {
    pub async fn forget(
        &self,
        platform: PlatformId,
        unmanaged: &dyn CredentialsRepo,
    ) -> Result<(), PlatformError> {
        match platform {
            PlatformId::Twitch => match &self.twitch {
                Some(manager) => manager.clear().await,
                None => delete_unmanaged(unmanaged, platform).await,
            },
            PlatformId::YouTube => match &self.youtube {
                Some(manager) => manager.clear().await,
                None => delete_unmanaged(unmanaged, platform).await,
            },
            PlatformId::Kick => match &self.kick {
                Some(manager) => manager.clear().await,
                None => delete_unmanaged(unmanaged, platform).await,
            },
        }
    }
}

async fn delete_unmanaged(
    repo: &dyn CredentialsRepo,
    platform: PlatformId,
) -> Result<(), PlatformError> {
    repo.delete(&CredentialId::new(credential_key(platform)))
        .await
        .map(drop)
        .map_err(|e| credential_storage_error(platform.as_str(), e))
}

fn credential_key(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::Twitch => forge_platform_twitch::TWITCH_CREDENTIAL_ID,
        PlatformId::YouTube => forge_platform_youtube::CREDENTIAL_KEY,
        PlatformId::Kick => forge_platform_kick::CREDENTIAL_KEY,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_events::{Event, EventPublisher};
    use forge_net::{RateLimiter, TokenBucketRateLimiter};
    use forge_platform_core::PlatformEndpoints;
    use forge_storage::StorageError;
    use time::OffsetDateTime;

    use super::*;

    const ROWS: [(PlatformId, &str); 3] = [
        (
            PlatformId::Twitch,
            forge_platform_twitch::TWITCH_CREDENTIAL_ID,
        ),
        (PlatformId::YouTube, forge_platform_youtube::CREDENTIAL_KEY),
        (PlatformId::Kick, forge_platform_kick::CREDENTIAL_KEY),
    ];

    struct Rows {
        rows: Mutex<BTreeMap<String, String>>,
        delete_fails: bool,
    }

    impl Rows {
        fn holding_every_platform() -> Arc<Self> {
            Self::new(false)
        }

        fn refusing_deletes() -> Arc<Self> {
            Self::new(true)
        }

        fn new(delete_fails: bool) -> Arc<Self> {
            let rows = ROWS
                .iter()
                .map(|(_, key)| ((*key).to_owned(), "{}".to_owned()))
                .collect();
            Arc::new(Self {
                rows: Mutex::new(rows),
                delete_fails,
            })
        }

        fn keys(&self) -> BTreeSet<String> {
            self.rows.lock().unwrap().keys().cloned().collect()
        }
    }

    #[async_trait]
    impl CredentialsRepo for Rows {
        async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
            self.rows
                .lock()
                .unwrap()
                .insert(id.as_str().to_owned(), plaintext.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.rows.lock().unwrap().get(id.as_str()).cloned())
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            if self.delete_fails {
                return Err(StorageError::Connection {
                    reason: "credentials store is read-only".to_owned(),
                });
            }
            Ok(self.rows.lock().unwrap().remove(id.as_str()).is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self.keys().into_iter().map(CredentialId::new).collect())
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

    struct Silent;

    impl EventPublisher for Silent {
        fn publish(&self, _: Event) {}
    }

    fn limiter() -> Arc<dyn RateLimiter> {
        Arc::new(TokenBucketRateLimiter::new(
            forge_platform_twitch::TWITCH_OAUTH_BUDGET_CAPACITY,
            forge_platform_twitch::TWITCH_OAUTH_BUDGET_WINDOW,
        ))
    }

    fn managers_over(
        repo: &Arc<Rows>,
        wired: impl Fn(PlatformId) -> bool,
    ) -> PlatformCredentialManagers {
        let endpoints = PlatformEndpoints::default();
        let repo = Arc::clone(repo) as Arc<dyn CredentialsRepo>;
        let twitch = forge_platform_twitch::TwitchCredentialsManager::new(
            forge_platform_twitch::TwitchOAuthClient::new(
                forge_platform_twitch::twitch_oauth_gateway(&endpoints, limiter()).unwrap(),
                "client".to_owned(),
            ),
            Arc::clone(&repo),
        );
        let youtube = forge_platform_youtube::YoutubeCredentialsManager::new(
            Arc::clone(&repo),
            forge_platform_youtube::GoogleOAuthClient::new(
                forge_platform_youtube::google_oauth_gateway(&endpoints, limiter()).unwrap(),
                "client".to_owned(),
                "secret".to_owned(),
            ),
            forge_platform_youtube::YoutubeDataApi::new(
                &endpoints,
                forge_platform_youtube::SharedQuota::default(),
                Arc::new(Silent),
            )
            .unwrap(),
        );
        PlatformCredentialManagers {
            twitch: wired(PlatformId::Twitch).then(|| Arc::new(twitch)),
            youtube: wired(PlatformId::YouTube).then(|| Arc::new(youtube)),
            kick: wired(PlatformId::Kick).then(|| kick_manager_over(repo)),
        }
    }

    pub(crate) fn kick_manager_over(
        repo: Arc<dyn CredentialsRepo>,
    ) -> Arc<forge_platform_kick::KickCredentialsManager> {
        let endpoints = PlatformEndpoints::default();
        Arc::new(forge_platform_kick::KickCredentialsManager::new(
            repo,
            forge_platform_kick::KickOAuthClient::new(
                &endpoints,
                forge_platform_kick::kick_oauth_gateway(&endpoints, limiter()).unwrap(),
                "client".to_owned(),
                "secret".to_owned(),
            ),
            forge_platform_kick::kick_api_gateway(&endpoints, limiter(), Arc::new(Silent)).unwrap(),
        ))
    }

    fn every_row() -> BTreeSet<String> {
        ROWS.iter().map(|(_, row)| (*row).to_owned()).collect()
    }

    fn every_row_but(key: &str) -> BTreeSet<String> {
        ROWS.iter()
            .map(|(_, row)| (*row).to_owned())
            .filter(|row| row != key)
            .collect()
    }

    #[tokio::test]
    async fn forget_clears_only_that_platforms_row_through_its_wired_manager() {
        for (platform, key) in ROWS {
            let managed = Rows::holding_every_platform();
            let unmanaged = Rows::holding_every_platform();
            let managers = managers_over(&managed, |_| true);

            managers.forget(platform, unmanaged.as_ref()).await.unwrap();

            assert_eq!(
                (managed.keys(), unmanaged.keys()),
                (every_row_but(key), every_row()),
                "{platform:?}"
            );
        }
    }

    #[tokio::test]
    async fn forget_deletes_the_row_directly_only_when_that_platform_has_no_manager() {
        for (platform, key) in ROWS {
            let managed = Rows::holding_every_platform();
            let unmanaged = Rows::holding_every_platform();
            let managers = managers_over(&managed, |wired| wired != platform);

            managers.forget(platform, unmanaged.as_ref()).await.unwrap();

            assert_eq!(
                (managed.keys(), unmanaged.keys()),
                (every_row(), every_row_but(key)),
                "{platform:?}"
            );
        }
    }

    #[tokio::test]
    async fn forget_reports_a_failed_delete_on_the_managed_and_the_direct_path() {
        for (platform, _) in ROWS {
            for wired in [true, false] {
                let refusing = Rows::refusing_deletes();
                let managers = managers_over(&refusing, |_| wired);

                let outcome = managers.forget(platform, refusing.as_ref()).await;

                assert!(
                    matches!(outcome, Err(PlatformError::Io(_))),
                    "{platform:?} wired={wired}: {outcome:?}"
                );
            }
        }
    }
}
