use forge_registry::{ChatTriggerFamily, TriggerRegistry};
use forge_storage::{ActionRepo, GlobalsRepo, HistoryRepo, StorageError, TriggerInstanceRepo};
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct DashboardStats {
    pub actions_count: usize,
    pub commands_count: usize,
    pub triggers_fired: u64,
    pub globals_count: usize,
}

pub async fn compute_stats(
    actions: &dyn ActionRepo,
    globals: &dyn GlobalsRepo,
    history: &dyn HistoryRepo,
    triggers: &dyn TriggerInstanceRepo,
    registry: &TriggerRegistry,
    since: OffsetDateTime,
) -> Result<DashboardStats, StorageError> {
    let actions_count = actions.count().await?;
    let globals_count = globals.count().await?;
    let command_kinds: Vec<String> = registry
        .all()
        .filter(|descriptor| descriptor.chat_trigger_family() == Some(ChatTriggerFamily::Command))
        .map(|descriptor| descriptor.id().to_owned())
        .collect();
    let commands_count = triggers.count_of_kinds(&command_kinds).await?;
    let stats = history.stats_summary(since).await?;
    let triggers_fired: u64 = stats.values().map(|s| u64::from(s.runs_24h)).sum();
    Ok(DashboardStats {
        actions_count,
        commands_count,
        triggers_fired,
        globals_count,
    })
}
