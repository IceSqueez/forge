use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    ActionRepo, ChatHistoryRepo, CredentialsRepo, EventLogRepo, GlobalsRepo, HistoryRepo,
    QueueRepo, ScriptRepo, SettingsRepo, SoundboardClipsRepo, StorageError, TriggerInstanceRepo,
    TtsFiltersRepo, TtsTriggerSettingsRepo, UserGlobalsRepo, ViewerRepo, VoiceAliasRepo,
    transit::{BundleExportOutcome, BundleImportOutcome, ImportMode},
};
use forge_types::ActionId;

/// Schema version this build expects. The startup gate compares `schema_version()`
/// against this constant; a mismatch routes to `Screen::SchemaUpgradeRequired`.
pub const EXPECTED_SCHEMA_VERSION: u32 = 30;

#[async_trait]
pub trait BundleRepo: Send + Sync {
    /// Impl owns deserialization + version gating. Only hard failures (malformed JSON,
    /// version below `MINIMUM_SUPPORTED_BUNDLE_VERSION`, DB write error) produce `Err`;
    /// soft conditions land in `BundleImportOutcome::warnings`.
    async fn import_bundle(
        &self,
        bytes: &[u8],
        mode: ImportMode,
    ) -> Result<BundleImportOutcome, StorageError>;

    /// Collects the full transitive closure from the root Action IDs. Missing deleted
    /// dependencies surface as warnings in the outcome; they do not abort the export.
    async fn export_bundle(
        &self,
        action_ids: &[ActionId],
        include_orphan_globals: bool,
    ) -> Result<BundleExportOutcome, StorageError>;
}

#[async_trait]
pub trait DataProvider:
    GlobalsRepo
    + UserGlobalsRepo
    + SettingsRepo
    + ScriptRepo
    + CredentialsRepo
    + BundleRepo
    + Send
    + Sync
{
    fn action_repo(&self) -> Arc<dyn ActionRepo>;
    fn trigger_instance_repo(&self) -> Arc<dyn TriggerInstanceRepo>;
    fn queue_repo(&self) -> Arc<dyn QueueRepo>;
    fn history_repo(&self) -> Arc<dyn HistoryRepo>;
    fn event_log_repo(&self) -> Arc<dyn EventLogRepo>;
    fn soundboard_clips_repo(&self) -> Arc<dyn SoundboardClipsRepo>;
    fn voice_alias_repo(&self) -> Arc<dyn VoiceAliasRepo>;
    fn viewer_repo(&self) -> Arc<dyn ViewerRepo>;
    fn tts_filters_repo(&self) -> Arc<dyn TtsFiltersRepo>;
    fn tts_trigger_settings_repo(&self) -> Arc<dyn TtsTriggerSettingsRepo>;
    fn chat_history_repo(&self) -> Arc<dyn ChatHistoryRepo>;

    /// Returns the number of migrations currently applied to the database.
    async fn schema_version(&self) -> Result<u32, StorageError>;

    /// Copies the underlying database file to `path`.
    async fn export(&self, path: &std::path::Path) -> Result<(), StorageError>;

    /// Closes the underlying connection pool. Must be awaited before the host runtime
    /// drops, otherwise sqlx's blocking workers hold the runtime's blocking pool open
    /// and `Runtime::drop` hangs indefinitely.
    async fn shutdown(&self);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _dyn(_: &dyn DataProvider) {}
}
