use super::core_settings::CORE_SETTINGS;
use super::portability::SettingPortability;
use super::row::{RowClasses, SettingRow};
use super::spec::{SettingKey, SettingSpec};
use super::typed::is_valid_member;
use crate::StorageError;

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

    #[cfg(feature = "settings-test-support")]
    pub fn specs(&self) -> &[SettingSpec] {
        &self.specs
    }

    #[cfg(feature = "settings-test-support")]
    pub fn spec_of(&self, stored_key: &str) -> Option<&SettingSpec> {
        self.find_spec(stored_key)
    }

    fn find_spec(&self, stored_key: &str) -> Option<&SettingSpec> {
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
        let spec = self.find_spec(key).ok_or(SettingSkip::Undeclared)?;
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

    fn declares_name(&self, name: &str) -> bool {
        self.specs.iter().any(|spec| spec.key.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::super::core_settings::{THEME, TTS_ENGINE_PARAMS};
    use super::super::disclosure::SettingDisclosure;
    use super::super::spec::SettingEncoding::{Flag, Json, Text};
    use super::*;

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
    fn an_unknown_key_is_undeclared_until_its_spec_is_registered() {
        let mut catalog = SettingsCatalog::core();
        let before = catalog.admit(EXTENSION_TEXT.key.name(), "nova");

        catalog.register(&[EXTENSION_TEXT]).unwrap();
        let after = catalog.admit(EXTENSION_TEXT.key.name(), "nova");

        assert_eq!(
            (before, after.map(|row| row.classes())),
            (
                Err(SettingSkip::Undeclared),
                Ok(RowClasses {
                    portability: SettingPortability::Portable,
                    disclosure: SettingDisclosure::Presence,
                }),
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
}
