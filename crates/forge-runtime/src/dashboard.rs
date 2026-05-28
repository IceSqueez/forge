use forge_storage::{ActionRepo, GlobalsRepo, HistoryRepo, StorageError};

#[derive(Debug, Clone)]
pub struct DashboardStats {
    pub actions_count: usize,
    pub triggers_fired: u64,
    pub globals_count: usize,
}

pub async fn compute_stats(
    actions: &dyn ActionRepo,
    globals: &dyn GlobalsRepo,
    history: &dyn HistoryRepo,
) -> Result<DashboardStats, StorageError> {
    let actions_count = actions.list().await?.len();
    let globals_count = globals.list().await?.len();
    let since = time::OffsetDateTime::now_utc() - time::Duration::hours(24);
    let stats = history.stats_summary(since).await?;
    let triggers_fired: u64 = stats.values().map(|s| u64::from(s.runs_24h)).sum();
    Ok(DashboardStats {
        actions_count,
        triggers_fired,
        globals_count,
    })
}
