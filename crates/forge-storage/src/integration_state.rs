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
