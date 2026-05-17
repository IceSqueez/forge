use async_trait::async_trait;

use crate::{
    ActionRepo, CommandRepo, CredentialsRepo, GlobalsRepo, HistoryRepo, QueueRepo, ScriptRepo,
    SettingsRepo, StorageError, TriggerRepo, UserGlobalsRepo,
};

#[async_trait]
pub trait DataProvider:
    GlobalsRepo
    + UserGlobalsRepo
    + SettingsRepo
    + ActionRepo
    + TriggerRepo
    + CommandRepo
    + QueueRepo
    + ScriptRepo
    + CredentialsRepo
    + HistoryRepo
    + Send
    + Sync
{
    /// Returns the number of migrations currently applied to the database.
    async fn schema_version(&self) -> Result<u32, StorageError>;

    /// Copies the underlying database file to `path`.
    async fn export(&self, path: &std::path::Path) -> Result<(), StorageError>;

    /// Replaces the current database with the file at `path`. Destructive.
    async fn import(&self, path: &std::path::Path) -> Result<(), StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _dyn(_: &dyn DataProvider) {}
}
