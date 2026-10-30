use forge_storage::{OptionalText, Setting, SettingsRepo, StorageError, read_setting};

pub(crate) async fn optional_text(
    repo: &dyn SettingsRepo,
    setting: &Setting<OptionalText>,
) -> Result<Option<String>, StorageError> {
    Ok(read_setting(repo, setting)
        .await?
        .filter(|value| !value.trim().is_empty()))
}
