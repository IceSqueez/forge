use std::future::Future;

use forge_types::IntegrationId;

use crate::settings::core_settings;
use crate::settings::typed::{read_stored_family_setting, write_family_setting};
use crate::{CredentialsRepo, SettingsRepo, StorageError};

pub fn integration_enabled_key(id: &IntegrationId) -> String {
    format!(
        "{}{}",
        core_settings::INTEGRATION_ENABLED.prefix(),
        id.as_str()
    )
}

fn readable_flag(result: Result<Option<bool>, StorageError>) -> Result<Option<bool>, StorageError> {
    match result {
        Err(StorageError::UndecodableSetting { key }) => {
            tracing::warn!(
                key,
                "stored integration switch cannot be decoded; seeding it again"
            );
            Ok(None)
        }
        other => other,
    }
}

pub(crate) async fn stored_integration_enabled(
    settings: &dyn SettingsRepo,
    id: &IntegrationId,
) -> Result<Option<bool>, StorageError> {
    readable_flag(
        read_stored_family_setting(settings, &core_settings::INTEGRATION_ENABLED, id.as_str())
            .await,
    )
}

pub async fn set_integration_enabled(
    settings: &dyn SettingsRepo,
    id: &IntegrationId,
    enabled: bool,
) -> Result<(), StorageError> {
    write_family_setting(
        settings,
        &core_settings::INTEGRATION_ENABLED,
        id.as_str(),
        &enabled,
    )
    .await
}

pub async fn resolve_integration_enabled<F>(
    settings: &dyn SettingsRepo,
    id: &IntegrationId,
    seed_when_unresolved: F,
) -> Result<bool, StorageError>
where
    F: Future<Output = Result<bool, StorageError>> + Send,
{
    if let Some(enabled) = stored_integration_enabled(settings, id).await? {
        return Ok(enabled);
    }
    let seeded = seed_when_unresolved.await?;
    set_integration_enabled(settings, id, seeded).await?;
    Ok(seeded)
}

pub async fn has_credentials_for(
    credentials: &dyn CredentialsRepo,
    id: &IntegrationId,
) -> Result<bool, StorageError> {
    let credential_namespace = format!("{}:", id.as_str());
    Ok(credentials
        .list_ids()
        .await?
        .iter()
        .any(|stored| stored.as_str().starts_with(&credential_namespace)))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use time::OffsetDateTime;

    use super::*;
    use crate::CredentialId;
    use crate::test_support::MemorySettings;

    fn settings_with(entries: &[(&str, &str)]) -> MemorySettings {
        let settings = MemorySettings::new();
        for (key, value) in entries {
            settings.seed_foreign(key, value, None);
        }
        settings
    }

    fn value_of(settings: &MemorySettings, key: &str) -> Option<String> {
        settings.row(key).map(|row| row.value)
    }

    struct StoredIds(Vec<&'static str>);

    #[async_trait]
    impl CredentialsRepo for StoredIds {
        async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(None)
        }
        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            Ok(false)
        }
        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self.0.iter().map(|id| CredentialId::new(*id)).collect())
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

    const NEW_TWITCH: &str = "integration.enabled.twitch";

    fn id(raw: &'static str) -> IntegrationId {
        IntegrationId::from_static(raw)
    }

    async fn probe(answer: bool, calls: &AtomicUsize) -> Result<bool, StorageError> {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(answer)
    }

    #[tokio::test]
    async fn an_unresolved_integration_seeds_from_the_probe_and_persists_the_answer() {
        for configured in [true, false] {
            let settings = MemorySettings::new();
            let calls = AtomicUsize::new(0);

            let enabled =
                resolve_integration_enabled(&settings, &id("twitch"), probe(configured, &calls))
                    .await
                    .unwrap();

            assert_eq!(enabled, configured);
            assert_eq!(
                value_of(&settings, NEW_TWITCH).as_deref(),
                Some(if configured { "true" } else { "false" })
            );
        }
    }

    #[tokio::test]
    async fn an_explicit_stored_choice_wins_over_the_probe() {
        for (stored, probed) in [("false", true), ("true", false)] {
            let settings = settings_with(&[(NEW_TWITCH, stored)]);
            let calls = AtomicUsize::new(0);

            let enabled =
                resolve_integration_enabled(&settings, &id("twitch"), probe(probed, &calls))
                    .await
                    .unwrap();

            assert_eq!(enabled, !probed, "stored {stored} must win");
            assert_eq!(calls.load(Ordering::SeqCst), 0, "the probe must not run");
        }
    }

    #[tokio::test]
    async fn resolving_again_reuses_the_seeded_answer_without_probing() {
        let settings = MemorySettings::new();
        let calls = AtomicUsize::new(0);

        let first = resolve_integration_enabled(&settings, &id("hotkey"), probe(false, &calls))
            .await
            .unwrap();
        let snapshot = settings.rows();
        let second = resolve_integration_enabled(&settings, &id("hotkey"), probe(true, &calls))
            .await
            .unwrap();

        assert_eq!((first, second), (false, false));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(settings.rows(), snapshot);
    }

    #[tokio::test]
    async fn an_unreadable_stored_value_is_reseeded_from_the_probe() {
        let settings = settings_with(&[(NEW_TWITCH, "maybe")]);
        let calls = AtomicUsize::new(0);

        let enabled = resolve_integration_enabled(&settings, &id("twitch"), probe(true, &calls))
            .await
            .unwrap();

        assert!(enabled);
        assert_eq!(value_of(&settings, NEW_TWITCH).as_deref(), Some("true"));
    }

    #[tokio::test]
    async fn a_failing_probe_persists_nothing_so_the_next_boot_retries() {
        let settings = MemorySettings::new();

        let result = resolve_integration_enabled(&settings, &id("twitch"), async {
            Err(StorageError::NotReady)
        })
        .await;

        assert!(result.is_err());
        assert_eq!(value_of(&settings, NEW_TWITCH), None);
    }

    #[tokio::test]
    async fn credentials_count_only_inside_the_integrations_own_namespace() {
        for (stored, expected) in [
            (vec!["twitch:broadcaster"], true),
            (vec!["twitchy:broadcaster", "kick:broadcaster"], false),
            (vec!["server:bearer", "twitch"], false),
            (vec![], false),
        ] {
            let repo = StoredIds(stored.clone());

            let found = has_credentials_for(&repo, &id("twitch")).await.unwrap();

            assert_eq!(found, expected, "{stored:?}");
        }
    }
}
