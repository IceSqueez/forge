use forge_storage::{SettingsRepo, StorageError};
use forge_widgets::{FontFamily, FontRole};

#[derive(Debug, Clone, Default)]
pub struct FontSettings {
    pub body: Option<String>,
    pub mono: Option<String>,
    pub catalog: Vec<FontFamily>,
    pub catalog_loaded: bool,
    pub mono_show_all: bool,
}

impl FontSettings {
    pub fn from_stored(body: Option<String>, mono: Option<String>) -> Self {
        Self {
            body,
            mono,
            ..Self::default()
        }
    }

    pub fn stored(&self, role: FontRole) -> Option<&str> {
        match role {
            FontRole::Body => self.body.as_deref(),
            FontRole::Monospace => self.mono.as_deref(),
        }
    }

    pub fn set_stored(&mut self, role: FontRole, family: Option<String>) {
        match role {
            FontRole::Body => self.body = family,
            FontRole::Monospace => self.mono = family,
        }
    }

    pub fn is_installed(&self, name: &str) -> bool {
        self.catalog.iter().any(|f| f.name == name)
    }

    /// Stored-but-not-installed family for the role; `None` until the catalog arrives.
    pub fn missing(&self, role: FontRole) -> Option<&str> {
        if self.catalog.is_empty() {
            return None;
        }
        self.stored(role).filter(|name| !self.is_installed(name))
    }
}

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

    fn catalog(names: &[(&str, bool)]) -> Vec<FontFamily> {
        names
            .iter()
            .map(|(name, monospaced)| FontFamily {
                name: (*name).to_owned(),
                monospaced: *monospaced,
            })
            .collect()
    }

    #[test]
    fn missing_is_none_until_the_catalog_arrives() {
        // Stored preference must not be flagged while enumeration is pending.
        let fonts = FontSettings::from_stored(Some("Vanished Sans".to_owned()), None);
        assert_eq!(fonts.missing(FontRole::Body), None);
    }

    #[test]
    fn missing_reports_stored_family_absent_from_catalog() {
        let mut fonts = FontSettings::from_stored(
            Some("Vanished Sans".to_owned()),
            Some("JetBrains Mono".to_owned()),
        );
        fonts.catalog = catalog(&[("Inter", false), ("JetBrains Mono", true)]);
        assert_eq!(fonts.missing(FontRole::Body), Some("Vanished Sans"));
        assert_eq!(fonts.missing(FontRole::Monospace), None);
    }

    #[test]
    fn missing_is_none_when_no_preference_is_stored() {
        let fonts = FontSettings {
            catalog: catalog(&[("Inter", false)]),
            ..FontSettings::default()
        };
        assert_eq!(fonts.missing(FontRole::Body), None);
        assert_eq!(fonts.missing(FontRole::Monospace), None);
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
