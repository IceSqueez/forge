use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    ActionRepo, CredentialsRepo, EventLogRepo, GlobalsRepo, HistoryRepo, QueueRepo, ScriptRepo,
    SettingsRepo, SoundboardClipsRepo, StorageError, TriggerInstanceRepo, UserGlobalsRepo,
    ViewerRepo, VoiceAliasRepo,
};

#[async_trait]
pub trait DataProvider:
    GlobalsRepo + UserGlobalsRepo + SettingsRepo + ScriptRepo + CredentialsRepo + Send + Sync
{
    fn action_repo(&self) -> Arc<dyn ActionRepo>;
    fn trigger_instance_repo(&self) -> Arc<dyn TriggerInstanceRepo>;
    fn queue_repo(&self) -> Arc<dyn QueueRepo>;
    fn history_repo(&self) -> Arc<dyn HistoryRepo>;
    fn event_log_repo(&self) -> Arc<dyn EventLogRepo>;
    fn soundboard_clips_repo(&self) -> Arc<dyn SoundboardClipsRepo>;
    fn voice_alias_repo(&self) -> Arc<dyn VoiceAliasRepo>;
    fn viewer_repo(&self) -> Arc<dyn ViewerRepo>;

    /// Returns the number of migrations currently applied to the database.
    async fn schema_version(&self) -> Result<u32, StorageError>;

    /// Copies the underlying database file to `path`.
    async fn export(&self, path: &std::path::Path) -> Result<(), StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _dyn(_: &dyn DataProvider) {}
}
