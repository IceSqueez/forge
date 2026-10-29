use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use super::core_settings::CORE_SETTINGS;
use super::disclosure::SettingDisclosure;
use super::portability::SettingPortability;
use super::spec::{SettingKey, SettingSpec};
use crate::{SettingsRepo, StorageError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsCatalog {
    specs: Vec<SettingSpec>,
}

static CORE_CATALOG: LazyLock<SettingsCatalog> = LazyLock::new(SettingsCatalog::core);

pub(crate) fn core_catalog() -> &'static SettingsCatalog {
    &CORE_CATALOG
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
