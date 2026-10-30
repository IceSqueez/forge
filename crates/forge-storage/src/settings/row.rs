use super::disclosure::SettingDisclosure;
use super::portability::SettingPortability;
use super::spec::SettingSpec;
use crate::{SettingsRepo, StorageError};

#[derive(Debug, Clone, Copy)]
pub struct RowGate(());

impl RowGate {
    pub(crate) const fn open() -> Self {
        Self(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowClasses {
    pub portability: SettingPortability,
    pub disclosure: SettingDisclosure,
}

impl RowClasses {
    pub const FAIL_CLOSED: Self = Self {
        portability: SettingPortability::MachineLocal,
        disclosure: SettingDisclosure::Withheld,
    };
}

impl From<&SettingSpec> for RowClasses {
    fn from(spec: &SettingSpec) -> Self {
        Self {
            portability: spec.portability,
            disclosure: spec.disclosure,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingRow {
    key: String,
    value: String,
    classes: RowClasses,
}

impl SettingRow {
    pub(crate) fn new(key: String, value: String, classes: RowClasses) -> Self {
        Self {
            key,
            value,
            classes,
        }
    }

    pub fn from_storage(_gate: RowGate, key: String, value: String, classes: RowClasses) -> Self {
        Self::new(key, value, classes)
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub const fn classes(&self) -> RowClasses {
        self.classes
    }
}

pub async fn load_setting_rows<R: SettingsRepo + ?Sized>(
    repo: &R,
) -> Result<Vec<SettingRow>, StorageError> {
    repo.load_rows(RowGate::open()).await
}

pub async fn restore_row<R: SettingsRepo + ?Sized>(
    repo: &R,
    row: &SettingRow,
) -> Result<(), StorageError> {
    repo.put_row(RowGate::open(), row.key(), row.value(), row.classes())
        .await
}

pub async fn remove_row<R: SettingsRepo + ?Sized>(
    repo: &R,
    row: &SettingRow,
) -> Result<bool, StorageError> {
    repo.delete(row.key()).await
}
