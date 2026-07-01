use std::sync::Arc;
use std::time::Duration;

use forge_storage::{
    ActionRepo, EventLogRepo, HistoryRepo, SettingsRepo, event_log_retention_days,
};
use time::OffsetDateTime;
use tokio::sync::Notify;

const MIN_EXECUTION_RETENTION_DAYS: u32 = 7;

pub(crate) fn spawn_retention_task(
    event_log: Arc<dyn EventLogRepo>,
    history: Arc<dyn HistoryRepo>,
    action: Arc<dyn ActionRepo>,
    settings: Arc<dyn SettingsRepo>,
    interval: Duration,
    shutdown: Arc<Notify>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let days = event_log_retention_days(settings.as_ref()).await.unwrap_or(7);
                    let now = OffsetDateTime::now_utc();
                    let cutoff = now - time::Duration::days(i64::from(days));
                    let exec_days = days.max(MIN_EXECUTION_RETENTION_DAYS);
                    let exec_cutoff = now - time::Duration::days(i64::from(exec_days));

                    match event_log.prune_before(cutoff).await {
                        Ok(pruned) => tracing::info!(
                            pruned_rows = pruned,
                            ?cutoff,
                            "event_log pruning complete"
                        ),
                        Err(e) => tracing::warn!(
                            error = %e,
                            "event_log pruning failed; will retry on next cycle"
                        ),
                    }

                    match history.prune_before(cutoff).await {
                        Ok(pruned) => tracing::info!(
                            pruned_rows = pruned,
                            ?cutoff,
                            "action_history pruning complete"
                        ),
                        Err(e) => tracing::warn!(
                            error = %e,
                            "action_history pruning failed; will retry on next cycle"
                        ),
                    }

                    match action.prune_executions_before(exec_cutoff).await {
                        Ok(pruned) => tracing::info!(
                            pruned_rows = pruned,
                            cutoff = ?exec_cutoff,
                            "action_executions pruning complete"
                        ),
                        Err(e) => tracing::warn!(
                            error = %e,
                            "action_executions pruning failed; will retry on next cycle"
                        ),
                    }
                }
                _ = shutdown.notified() => {
                    tracing::info!("retention pruner stopped");
                    break;
                }
            }
        }
    })
}
