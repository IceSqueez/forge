use forge_storage::{SettingsRepo, StorageError};

/// Maps the storage-side density onto the widget-side token cell; call on the main thread.
pub fn install_density(density: forge_storage::settings::Density) {
    let widget_density = match density {
        forge_storage::settings::Density::Compact => forge_widgets::Density::Compact,
        forge_storage::settings::Density::Cozy => forge_widgets::Density::Cozy,
        forge_storage::settings::Density::Spacious => forge_widgets::Density::Spacious,
    };
    forge_widgets::install_density(widget_density);
}

pub async fn sheet_width(repo: &dyn SettingsRepo, key: &str) -> Result<Option<f32>, StorageError> {
    let storage_key = format!("sheet_width:{key}");
    let raw = repo.get_string(&storage_key).await?;
    match raw {
        None => Ok(None),
        Some(s) => match s.parse::<f32>() {
            Ok(v) => Ok(Some(v)),
            Err(_) => {
                tracing::warn!(key, raw = %s, "sheet_width: stored value is not a valid f32; falling back to None");
                Ok(None)
            }
        },
    }
}

pub async fn set_sheet_width(
    repo: &dyn SettingsRepo,
    key: &str,
    width: f32,
) -> Result<(), StorageError> {
    let storage_key = format!("sheet_width:{key}");
    repo.set_string(&storage_key, &width.to_string()).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use forge_storage::SettingsRepo;
    use forge_storage_sqlite::SqliteBackend;

    use super::*;

    const TEST_KEY: [u8; 32] = [0xab; 32];

    async fn setup() -> SqliteBackend {
        SqliteBackend::open_with_key(":memory:", TEST_KEY)
            .await
            .expect("in-memory backend")
    }

    #[tokio::test]
    async fn sheet_width_roundtrip() {
        let backend = setup().await;
        set_sheet_width(&backend, "viewers_drawer", 420.0)
            .await
            .expect("set sheet_width");
        let got = sheet_width(&backend, "viewers_drawer")
            .await
            .expect("get sheet_width");
        assert_eq!(got, Some(420.0_f32));
    }

    #[tokio::test]
    async fn sheet_width_absent_key_returns_none() {
        let backend = setup().await;
        let got = sheet_width(&backend, "no_such_sheet")
            .await
            .expect("absent key");
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn sheet_width_corrupt_value_returns_none() {
        let backend = setup().await;
        backend
            .set_string("sheet_width:corrupt_key", "not_a_float")
            .await
            .expect("inject corrupt value");
        let got = sheet_width(&backend, "corrupt_key")
            .await
            .expect("corrupt value fallback");
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn sheet_width_keys_do_not_collide() {
        let backend = setup().await;
        set_sheet_width(&backend, "action_editor", 480.0)
            .await
            .expect("set action_editor");
        set_sheet_width(&backend, "trigger_editor", 360.0)
            .await
            .expect("set trigger_editor");

        let action = sheet_width(&backend, "action_editor")
            .await
            .expect("get action_editor");
        let trigger = sheet_width(&backend, "trigger_editor")
            .await
            .expect("get trigger_editor");

        assert_eq!(action, Some(480.0_f32));
        assert_eq!(trigger, Some(360.0_f32));
    }
}
