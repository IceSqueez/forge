use std::path::Path;

use crate::error::StorageError;

pub trait DatabaseFileSchema: Send + Sync {
    fn applied_schema_version(&self, database_file: &Path) -> Result<Option<u32>, StorageError>;
}
