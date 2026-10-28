use forge_storage::{SettingsRepo, StorageError};

pub(crate) async fn text(
    repo: &dyn SettingsRepo,
    key: &str,
) -> Result<Option<String>, StorageError> {
    Ok(repo
        .get_string(key)
        .await?
        .filter(|value| !value.trim().is_empty()))
}

pub(crate) async fn bounded(
    repo: &dyn SettingsRepo,
    key: &str,
    (min, default, max): (u32, u32, u32),
) -> Result<u32, StorageError> {
    Ok(text(repo, key)
        .await?
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .map_or(default, |value| value.clamp(min, max)))
}
