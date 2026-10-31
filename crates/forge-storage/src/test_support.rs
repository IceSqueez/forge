mod credentials;
mod memory;

pub use credentials::MemoryCredentials;
pub use memory::MemorySettings;

use crate::{RowClasses, RowGate};

pub const OPEN_ROW_GATE: RowGate = RowGate::open();

pub const FAIL_CLOSED_CLASSES: RowClasses = RowClasses::FAIL_CLOSED;

pub const INJECTED_FAILURE_REASON: &str = "injected settings failure";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSettingRow {
    pub key: String,
    pub value: String,
    pub classes: RowClasses,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsWrite {
    Put {
        key: String,
        value: String,
        classes: RowClasses,
    },
    Delete {
        key: String,
    },
}
