use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    ActionRepo, BanLedgerRepo, CatalogRevision, ChatHistoryRepo, CredentialsRepo, DonationRepo,
    EventLogRepo, GlobalsRepo, HistoryRepo, LatestValueRepo, MediaRepo, OverlayRepo, QueueRepo,
    ScheduledRunRepo, ScriptRepo, SettingsRepo, SoundboardClipsRepo, StorageError,
    TriggerInstanceRepo, TtsFiltersRepo, UserGlobalsRepo, ViewerRepo, VoiceAliasRepo,
};

pub const EXPECTED_SCHEMA_VERSION: u32 = 52;

pub const LAST_PRE_BASELINE_RELEASE: &str = "0.5.5";

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
    fn media_repo(&self) -> Arc<dyn MediaRepo>;
    fn donation_repo(&self) -> Arc<dyn DonationRepo>;
    fn latest_value_repo(&self) -> Arc<dyn LatestValueRepo>;
    fn ban_ledger_repo(&self) -> Arc<dyn BanLedgerRepo>;

    fn scheduled_run_repo(&self) -> Arc<dyn ScheduledRunRepo>;

    fn catalog_revision(&self) -> CatalogRevision;

    fn scheduled_run_revision(&self) -> CatalogRevision;

    async fn export(&self, path: &std::path::Path) -> Result<(), StorageError>;

    async fn shutdown(&self);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _dyn(_: &dyn DataProvider) {}
}
