use std::path::Path;

use crate::error::StorageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredSchema {
    Current { version: u32 },
    AdoptableLegacy { version: u32 },
    PreBaseline { found: u32 },
    Unrecognized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredDatabase {
    pub schema: StoredSchema,
    pub holds_credentials: bool,
}

pub trait DatabaseFileSchema: Send + Sync {
    fn applied_schema_version(&self, database_file: &Path) -> Result<Option<u32>, StorageError>;

    fn inspect(&self, database_file: &Path) -> Result<Option<StoredDatabase>, StorageError>;
}
