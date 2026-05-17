use async_trait::async_trait;

use crate::{
    ActionRepo, CommandRepo, CredentialsRepo, GlobalsRepo, HistoryRepo, QueueRepo, ScriptRepo,
    SettingsRepo, StorageError, TriggerRepo, UserGlobalsRepo,
};

#[async_trait]
pub trait DataProvider:
    GlobalsRepo + UserGlobalsRepo + SettingsRepo + ScriptRepo + CredentialsRepo + Send + Sync
{
    fn action_repo(&self) -> &dyn ActionRepo;
    fn trigger_repo(&self) -> &dyn TriggerRepo;
    fn command_repo(&self) -> &dyn CommandRepo;
    fn queue_repo(&self) -> &dyn QueueRepo;
    fn history_repo(&self) -> &dyn HistoryRepo;

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
