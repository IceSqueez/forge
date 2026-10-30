use super::disclosure::SettingDisclosure;
use super::portability::SettingPortability;
use super::spec::SettingSpec;

#[derive(Debug)]
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

impl From<&SettingSpec> for RowClasses {
    fn from(spec: &SettingSpec) -> Self {
        Self {
            portability: spec.portability,
            disclosure: spec.disclosure,
        }
    }
}
