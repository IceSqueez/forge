use std::future::Future;

use forge_types::IntegrationId;

use crate::settings::{decode_bool_setting, reserved_keys};
use crate::{CredentialsRepo, SettingsRepo, StorageError};

pub fn integration_enabled_key(id: &IntegrationId) -> String {
    format!(
        "{}{}",
        reserved_keys::INTEGRATION_ENABLED_PREFIX,
        id.as_str()
    )
}

fn retired_enabled_key_of(id: &IntegrationId) -> Option<&'static str> {
    match id.as_str() {
        "midi" => Some(reserved_keys::RETIRED_MIDI_ENABLED),
        "hotkey" => Some(reserved_keys::RETIRED_HOTKEY_ENABLED),
        _ => None,
    }
}

pub async fn stored_integration_enabled(
    settings: &dyn SettingsRepo,
    id: &IntegrationId,
) -> Result<Option<bool>, StorageError> {
    Ok(settings
        .get_string(&integration_enabled_key(id))
        .await?
        .as_deref()
        .and_then(decode_bool_setting))
}

pub async fn set_integration_enabled(
    settings: &dyn SettingsRepo,
    id: &IntegrationId,
    enabled: bool,
) -> Result<(), StorageError> {
    settings
        .set_string(
            &integration_enabled_key(id),
            if enabled { "true" } else { "false" },
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
    let retired_key = retired_enabled_key_of(id);
    let resolved = match stored_integration_enabled(settings, id).await? {
        Some(enabled) => enabled,
        None => {
            let retired_value = match retired_key {
                Some(key) => settings
                    .get_string(key)
                    .await?
                    .as_deref()
                    .and_then(decode_bool_setting),
                None => None,
            };
            let seeded = match retired_value {
                Some(enabled) => enabled,
                None => seed_when_unresolved.await?,
            };
            set_integration_enabled(settings, id, seeded).await?;
            seeded
        }
    };
    if let Some(key) = retired_key {
        settings.delete(key).await?;
    }
    Ok(resolved)
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

pub async fn has_setting(settings: &dyn SettingsRepo, key: &str) -> Result<bool, StorageError> {
    Ok(settings.get_string(key).await?.is_some())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use time::OffsetDateTime;

    use super::*;
    use crate::CredentialId;

    #[derive(Default)]
    struct MemorySettings(Mutex<HashMap<String, String>>);

    impl MemorySettings {
        fn with(entries: &[(&str, &str)]) -> Self {
            Self(Mutex::new(
                entries
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                    .collect(),
            ))
        }

        fn value(&self, key: &str) -> Option<String> {
            self.0.lock().unwrap().get(key).cloned()
        }
    }

    #[async_trait]
    impl SettingsRepo for MemorySettings {
        async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
            Ok(self.value(key))
        }
        async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
            self.0
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }
        async fn delete(&self, key: &str) -> Result<bool, StorageError> {
            Ok(self.0.lock().unwrap().remove(key).is_some())
        }
        async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
            Ok(self.0.lock().unwrap().clone())
        }
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
    const NEW_MIDI: &str = "integration.enabled.midi";
    const NEW_HOTKEY: &str = "integration.enabled.hotkey";

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
            let settings = MemorySettings::default();
            let calls = AtomicUsize::new(0);

            let enabled =
                resolve_integration_enabled(&settings, &id("twitch"), probe(configured, &calls))
                    .await
                    .unwrap();

            assert_eq!(enabled, configured);
            assert_eq!(
                settings.value(NEW_TWITCH).as_deref(),
                Some(if configured { "true" } else { "false" })
            );
        }
    }

    #[tokio::test]
    async fn an_explicit_stored_choice_wins_over_the_probe() {
        for (stored, probed) in [("false", true), ("true", false)] {
            let settings = MemorySettings::with(&[(NEW_TWITCH, stored)]);
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
    async fn a_legacy_switch_seeds_the_new_state_and_is_deleted() {
        for (integration, legacy, new_key, legacy_value, expected) in [
            ("midi", "midi.enabled", NEW_MIDI, "false", false),
            ("hotkey", "hotkey.enabled", NEW_HOTKEY, "true", true),
        ] {
            let settings = MemorySettings::with(&[(legacy, legacy_value)]);
            let calls = AtomicUsize::new(0);

            let enabled =
                resolve_integration_enabled(&settings, &id(integration), probe(!expected, &calls))
                    .await
                    .unwrap();

            assert_eq!(enabled, expected, "{integration}");
            assert_eq!(calls.load(Ordering::SeqCst), 0, "{integration}: probe ran");
            assert_eq!(settings.value(new_key), Some(expected.to_string()));
            assert_eq!(
                settings.value(legacy),
                None,
                "{integration}: legacy key kept"
            );
        }
    }

    #[tokio::test]
    async fn a_legacy_switch_left_beside_a_stored_choice_is_deleted_without_overriding_it() {
        let settings = MemorySettings::with(&[(NEW_MIDI, "true"), ("midi.enabled", "false")]);
        let calls = AtomicUsize::new(0);

        let enabled = resolve_integration_enabled(&settings, &id("midi"), probe(false, &calls))
            .await
            .unwrap();

        assert!(enabled);
        assert_eq!(settings.value("midi.enabled"), None);
    }

    #[tokio::test]
    async fn a_legacy_key_of_another_integration_is_neither_read_nor_deleted() {
        let settings = MemorySettings::with(&[("midi.enabled", "true")]);
        let calls = AtomicUsize::new(0);

        let enabled = resolve_integration_enabled(&settings, &id("twitch"), probe(false, &calls))
            .await
            .unwrap();

        assert!(!enabled);
        assert_eq!(settings.value("midi.enabled").as_deref(), Some("true"));
    }

    #[tokio::test]
    async fn resolving_again_reuses_the_seeded_answer_without_probing() {
        let settings = MemorySettings::with(&[("hotkey.enabled", "false")]);
        let calls = AtomicUsize::new(0);

        let first = resolve_integration_enabled(&settings, &id("hotkey"), probe(true, &calls))
            .await
            .unwrap();
        let snapshot = settings.load_all().await.unwrap();
        let second = resolve_integration_enabled(&settings, &id("hotkey"), probe(true, &calls))
            .await
            .unwrap();

        assert_eq!((first, second), (false, false));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(settings.load_all().await.unwrap(), snapshot);
    }

    #[tokio::test]
    async fn an_unreadable_stored_value_is_reseeded_from_the_probe() {
        let settings = MemorySettings::with(&[(NEW_TWITCH, "maybe")]);
        let calls = AtomicUsize::new(0);

        let enabled = resolve_integration_enabled(&settings, &id("twitch"), probe(true, &calls))
            .await
            .unwrap();

        assert!(enabled);
        assert_eq!(settings.value(NEW_TWITCH).as_deref(), Some("true"));
    }

    #[tokio::test]
    async fn a_failing_probe_persists_nothing_so_the_next_boot_retries() {
        let settings = MemorySettings::default();

        let result = resolve_integration_enabled(&settings, &id("twitch"), async {
            Err(StorageError::NotReady)
        })
        .await;

        assert!(result.is_err());
        assert_eq!(settings.value(NEW_TWITCH), None);
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
