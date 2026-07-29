use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    ActionRepo, ChatHistoryRepo, CredentialsRepo, EventLogRepo, GlobalsRepo, HistoryRepo,
    OverlayRepo, QueueRepo, ScriptRepo, SettingsRepo, SoundboardClipsRepo, StorageError,
    TriggerInstanceRepo, TtsFiltersRepo, UserGlobalsRepo, ViewerRepo, VoiceAliasRepo,
};

/// Schema version this build expects. The startup gate compares `schema_version()`
/// against this constant; a mismatch routes to `Screen::SchemaUpgradeRequired`.
pub const EXPECTED_SCHEMA_VERSION: u32 = 39;

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
    fn tts_filters_repo(&self) -> Arc<dyn TtsFiltersRepo>;
    fn chat_history_repo(&self) -> Arc<dyn ChatHistoryRepo>;
    fn overlay_repo(&self) -> Arc<dyn OverlayRepo>;

    async fn schema_version(&self) -> Result<u32, StorageError>;

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
