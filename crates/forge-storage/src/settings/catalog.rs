use std::collections::{BTreeMap, HashMap};

use super::core_settings::CORE_SETTINGS;
use super::disclosure::SettingDisclosure;
use super::portability::SettingPortability;
use super::row::{RowClasses, SettingRow};
use super::spec::{SettingKey, SettingSpec};
use super::typed::is_valid_member;
use crate::{SettingsRepo, StorageError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingSkip {
    Undeclared,
    NotPortable,
    Undecodable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsCatalog {
    specs: Vec<SettingSpec>,
}

impl SettingsCatalog {
    pub fn core() -> Self {
        Self {
            specs: CORE_SETTINGS.to_vec(),
        }
    }

    pub fn register(&mut self, specs: &[SettingSpec]) -> Result<(), StorageError> {
        for (position, spec) in specs.iter().enumerate() {
            let name = spec.key.name();
            let declared_here = specs[..position]
                .iter()
                .any(|earlier| earlier.key.name() == name);
            if declared_here || self.declares_name(name) {
                return Err(StorageError::DuplicateSettingSpec {
                    key: name.to_owned(),
                });
            }
        }
        self.specs.extend_from_slice(specs);
        Ok(())
    }

    pub fn specs(&self) -> &[SettingSpec] {
        &self.specs
    }

    pub fn spec_of(&self, stored_key: &str) -> Option<&SettingSpec> {
        self.specs
            .iter()
            .find(|spec| matches!(spec.key, SettingKey::Exact(name) if name == stored_key))
            .or_else(|| {
                self.specs
                    .iter()
                    .filter(|spec| matches!(spec.key, SettingKey::Prefix(_)))
                    .filter(|spec| spec.key.matches(stored_key))
                    .max_by_key(|spec| spec.key.name().len())
            })
    }

    pub fn admit(&self, key: &str, value: &str) -> Result<SettingRow, SettingSkip> {
        let spec = self.spec_of(key).ok_or(SettingSkip::Undeclared)?;
        if let SettingKey::Prefix(prefix) = spec.key
            && !key.strip_prefix(prefix).is_some_and(is_valid_member)
        {
            return Err(SettingSkip::Undeclared);
        }
        if spec.portability != SettingPortability::Portable {
            return Err(SettingSkip::NotPortable);
        }
        if !spec.encoding.accepts(value) {
            return Err(SettingSkip::Undecodable);
        }
        Ok(SettingRow::new(
            key.to_owned(),
            value.to_owned(),
            RowClasses::from(spec),
        ))
    }

    pub fn portability_of(&self, stored_key: &str) -> SettingPortability {
        self.spec_of(stored_key)
            .map_or(SettingPortability::MachineLocal, |spec| spec.portability)
    }

    pub fn is_portable(&self, stored_key: &str) -> bool {
        self.portability_of(stored_key) == SettingPortability::Portable
    }

    pub fn disclosure_of(&self, stored_key: &str) -> SettingDisclosure {
        self.spec_of(stored_key)
            .map_or(SettingDisclosure::Withheld, |spec| spec.disclosure)
    }

    pub fn disclose_all(&self, stored: &HashMap<String, String>) -> BTreeMap<String, String> {
        stored
            .iter()
            .filter_map(|(key, value)| {
                self.disclosure_of(key)
                    .render(value)
                    .map(|shown| (key.clone(), shown))
            })
            .collect()
    }

    pub fn undeclared_keys<'a>(
        &self,
        stored_keys: impl IntoIterator<Item = &'a str>,
    ) -> Vec<&'a str> {
        let mut undeclared: Vec<&str> = stored_keys
            .into_iter()
            .filter(|key| self.spec_of(key).is_none())
            .collect();
        undeclared.sort_unstable();
        undeclared
    }

    fn declares_name(&self, name: &str) -> bool {
        self.specs.iter().any(|spec| spec.key.name() == name)
    }
}

pub async fn report_undeclared_settings(
    repo: &dyn SettingsRepo,
    catalog: &SettingsCatalog,
) -> Result<Vec<String>, StorageError> {
    let stored = repo.load_all().await?;
    let undeclared: Vec<String> = catalog
        .undeclared_keys(stored.keys().map(String::as_str))
        .into_iter()
        .map(str::to_owned)
        .collect();
    for key in &undeclared {
        tracing::warn!(key = %key, "stored setting has no declared spec");
    }
    Ok(undeclared)
}

#[cfg(test)]
mod tests {
    use super::super::core_settings::{
        ACCENT, AUDIO_OUTPUT_DEVICE_ID, BACKUP_SCHEDULE_DESTINATION, PICKER_FAVORITES_ICONS, THEME,
        TTS_ENGINE_PARAMS,
    };
    use super::super::spec::SettingEncoding::{Flag, Json, Text};
    use super::*;
    use crate::test_support::MemorySettings;

    const EXTENSION_FLAG: SettingSpec = SettingSpec::exact(
        "extension.flag",
        Flag,
        SettingPortability::Portable,
        SettingDisclosure::Verbatim,
    );
    const EXTENSION_TEXT: SettingSpec = SettingSpec::exact(
        "extension.text",
        Text,
        SettingPortability::Portable,
        SettingDisclosure::Presence,
    );

    fn duplicate_key(result: Result<(), StorageError>) -> Option<String> {
        match result {
            Err(StorageError::DuplicateSettingSpec { key }) => Some(key),
            _ => None,
        }
    }

    #[test]
    fn register_rejects_a_name_the_core_catalog_already_declares() {
        for spec in [
            SettingSpec::exact(
                THEME.name(),
                Text,
                SettingPortability::MachineLocal,
                SettingDisclosure::Withheld,
            ),
            SettingSpec::prefix(
                THEME.name(),
                Text,
                SettingPortability::Portable,
                SettingDisclosure::Verbatim,
            ),
            SettingSpec::exact(
                TTS_ENGINE_PARAMS.prefix(),
                Json,
                SettingPortability::Portable,
                SettingDisclosure::Verbatim,
            ),
        ] {
            let mut catalog = SettingsCatalog::core();

            let rejected = duplicate_key(catalog.register(&[spec]));

            assert_eq!(rejected.as_deref(), Some(spec.key.name()), "{spec:?}");
        }
    }

    #[test]
    fn register_rejects_a_name_declared_by_an_earlier_registration() {
        let mut catalog = SettingsCatalog::core();
        catalog.register(&[EXTENSION_FLAG]).unwrap();

        let rejected = duplicate_key(catalog.register(&[EXTENSION_FLAG]));

        assert_eq!(rejected.as_deref(), Some("extension.flag"));
    }

    #[test]
    fn register_rejects_a_name_repeated_within_one_slice_and_declares_nothing_from_it() {
        let mut catalog = SettingsCatalog::core();

        let rejected =
            duplicate_key(catalog.register(&[EXTENSION_TEXT, EXTENSION_FLAG, EXTENSION_FLAG]));

        assert_eq!(
            (rejected.as_deref(), catalog),
            (Some("extension.flag"), SettingsCatalog::core())
        );
    }

    #[test]
    fn an_unknown_key_is_machine_local_and_withheld_until_its_spec_is_registered() {
        let mut catalog = SettingsCatalog::core();
        let before = (
            catalog.portability_of(EXTENSION_TEXT.key.name()),
            catalog.disclosure_of(EXTENSION_TEXT.key.name()),
            catalog.undeclared_keys([EXTENSION_TEXT.key.name()]),
        );

        catalog.register(&[EXTENSION_TEXT]).unwrap();
        let after = (
            catalog.portability_of(EXTENSION_TEXT.key.name()),
            catalog.disclosure_of(EXTENSION_TEXT.key.name()),
            catalog.undeclared_keys([EXTENSION_TEXT.key.name()]),
        );

        assert_eq!(
            (before, after),
            (
                (
                    SettingPortability::MachineLocal,
                    SettingDisclosure::Withheld,
                    vec!["extension.text"],
                ),
                (
                    SettingPortability::Portable,
                    SettingDisclosure::Presence,
                    Vec::new(),
                ),
            )
        );
    }

    #[test]
    fn spec_of_prefers_an_exact_name_then_the_longest_matching_prefix() {
        let mut catalog = SettingsCatalog::core();
        catalog
            .register(&[
                SettingSpec::prefix(
                    "ext.deep.",
                    Json,
                    SettingPortability::MachineLocal,
                    SettingDisclosure::Kind,
                ),
                SettingSpec::prefix(
                    "ext.",
                    Text,
                    SettingPortability::Portable,
                    SettingDisclosure::Verbatim,
                ),
                SettingSpec::exact(
                    "ext.deep.pinned",
                    Flag,
                    SettingPortability::Portable,
                    SettingDisclosure::Withheld,
                ),
            ])
            .unwrap();

        for (stored_key, resolved) in [
            (
                "ext.deep.pinned",
                Some(SettingKey::Exact("ext.deep.pinned")),
            ),
            ("ext.deep.other", Some(SettingKey::Prefix("ext.deep."))),
            ("ext.deep.", Some(SettingKey::Prefix("ext.deep."))),
            ("ext.deep", Some(SettingKey::Prefix("ext."))),
            ("ext.other", Some(SettingKey::Prefix("ext."))),
            ("ext", None),
            ("EXT.other", None),
        ] {
            assert_eq!(
                catalog.spec_of(stored_key).map(|spec| spec.key),
                resolved,
                "key {stored_key:?}",
            );
        }
    }

    #[test]
    fn undeclared_keys_lists_only_unmatched_keys_in_sorted_order() {
        let catalog = SettingsCatalog::core();

        let undeclared = catalog.undeclared_keys([
            "zeta.unknown",
            THEME.name(),
            "tts.engine_params.piper",
            "alpha.unknown",
            "tts.engine_params",
        ]);

        assert_eq!(
            undeclared,
            ["alpha.unknown", "tts.engine_params", "zeta.unknown"]
        );
    }

    #[test]
    fn disclose_all_drops_withheld_keys_and_renders_the_rest_by_their_class() {
        let stored: HashMap<String, String> = [
            (ACCENT.name(), "blue"),
            (AUDIO_OUTPUT_DEVICE_ID.name(), "nova-speakers"),
            (PICKER_FAVORITES_ICONS.name(), r#"["nova","star"]"#),
            (BACKUP_SCHEDULE_DESTINATION.name(), "/home/nova/backups"),
            ("twitch.account_login", "nova"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();

        let shown = SettingsCatalog::core().disclose_all(&stored);

        assert_eq!(
            shown,
            BTreeMap::from([
                (ACCENT.name().to_owned(), "blue".to_owned()),
                (AUDIO_OUTPUT_DEVICE_ID.name().to_owned(), "set".to_owned()),
                (
                    PICKER_FAVORITES_ICONS.name().to_owned(),
                    "list(2)".to_owned()
                ),
            ])
        );
    }

    #[tokio::test]
    async fn report_undeclared_settings_returns_the_undeclared_keys_and_leaves_the_store_untouched()
    {
        let repo = MemorySettings::new();
        for (stored_key, value) in [
            (THEME.name(), "catppuccin-mocha"),
            ("legacy.retired_toggle", "true"),
            ("integration.enabled.twitch", "false"),
            ("another.stray", "{}"),
        ] {
            repo.seed_foreign(stored_key, value, None);
        }
        let stored_before = repo.rows();

        let reported = report_undeclared_settings(&repo, &SettingsCatalog::core())
            .await
            .unwrap();

        assert_eq!(
            (reported, repo.rows()),
            (
                vec![
                    "another.stray".to_owned(),
                    "legacy.retired_toggle".to_owned()
                ],
                stored_before,
            )
        );
    }
}
