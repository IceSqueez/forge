use std::sync::Arc;
use std::time::Duration;

use forge_storage::{EventLogRepo, SettingsRepo};
use time::OffsetDateTime;
use tokio::sync::Notify;

pub(crate) fn spawn_retention_task(
    repo: Arc<dyn EventLogRepo>,
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
                    let days = settings.event_log_retention_days().await.unwrap_or(7);
                    let cutoff = OffsetDateTime::now_utc()
                        - time::Duration::days(i64::from(days));
                    match repo.prune_before(cutoff).await {
                        Ok(pruned) => {
                            tracing::info!(
                                pruned_rows = pruned,
                                ?cutoff,
                                "event_log pruning complete"
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "event_log pruning failed; will retry on next cycle"
                            );
                        }
                    }
                }
                _ = shutdown.notified() => {
                    tracing::info!("event_log retention pruner stopped");
                    break;
                }
            }
        }
    })
}
