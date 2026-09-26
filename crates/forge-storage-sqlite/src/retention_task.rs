use std::sync::Arc;
use std::time::Duration;

use forge_storage::{
    ActionRepo, DEFAULT_EVENT_LOG_RETENTION_DAYS, HistoryRepo, SettingsRepo,
    event_log_retention_days,
};
use time::OffsetDateTime;
use tokio::sync::Notify;

use crate::SqliteEventLogRepo;

const MIN_EXECUTION_RETENTION_DAYS: u32 = 7;
const FIRST_SWEEP_DELAY: Duration = Duration::from_secs(30);
const EVENT_LOG_PRUNE_CHUNK_ROWS: u32 = 1_000;
const EVENT_LOG_PRUNE_CHUNK_GAP: Duration = Duration::from_millis(100);

pub(crate) struct RetentionSignals {
    pub(crate) window_changed: Arc<Notify>,
    pub(crate) shutdown: Arc<Notify>,
}

pub(crate) struct RetentionTargets {
    pub(crate) event_log: Arc<SqliteEventLogRepo>,
    pub(crate) history: Arc<dyn HistoryRepo>,
    pub(crate) action: Arc<dyn ActionRepo>,
    pub(crate) settings: Arc<dyn SettingsRepo>,
}

enum Interrupt {
    WindowChanged,
    Shutdown,
}

pub(crate) fn spawn_retention_task(
    targets: RetentionTargets,
    interval: Duration,
    signals: RetentionSignals,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(interval.min(FIRST_SWEEP_DELAY)) => {}
            _ = signals.shutdown.notified() => return,
        }
        let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut days = retention_days(targets.settings.as_ref()).await;

        loop {
            match sweep(&targets, days, &signals).await {
                Ok(()) => {}
                Err(Interrupt::WindowChanged) => {
                    days = retention_days(targets.settings.as_ref()).await;
                    continue;
                }
                Err(Interrupt::Shutdown) => break,
            }
            tokio::select! {
                _ = ticker.tick() => {}
                _ = signals.window_changed.notified() => {
                    days = retention_days(targets.settings.as_ref()).await;
                }
                _ = signals.shutdown.notified() => break,
            }
        }
        tracing::info!("retention pruner stopped");
    })
}

async fn retention_days(settings: &dyn SettingsRepo) -> u32 {
    event_log_retention_days(settings)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "could not read the event history retention; using the default");
            DEFAULT_EVENT_LOG_RETENTION_DAYS
        })
}

async fn sweep(
    targets: &RetentionTargets,
    days: u32,
    signals: &RetentionSignals,
) -> Result<(), Interrupt> {
    let now = OffsetDateTime::now_utc();
    let cutoff = now - time::Duration::days(i64::from(days));
    let exec_cutoff = now - time::Duration::days(i64::from(days.max(MIN_EXECUTION_RETENTION_DAYS)));

    prune_event_log(&targets.event_log, cutoff, signals).await?;

    match targets.history.prune_before(cutoff).await {
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

    match targets.action.prune_executions_before(exec_cutoff).await {
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
    Ok(())
}

/// Chunks share the single writer with batched inserts and the WAL restart, so each one commits
/// alone and the gap lets queued writes and checkpoints run before the next.
async fn prune_event_log(
    event_log: &SqliteEventLogRepo,
    cutoff: OffsetDateTime,
    signals: &RetentionSignals,
) -> Result<(), Interrupt> {
    let mut pruned = 0u64;
    loop {
        match event_log
            .prune_chunk_before(cutoff, EVENT_LOG_PRUNE_CHUNK_ROWS)
            .await
        {
            Ok(rows) => {
                pruned += rows;
                if rows < u64::from(EVENT_LOG_PRUNE_CHUNK_ROWS) {
                    break;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, pruned_rows = pruned, "event_log pruning failed; will retry on next cycle");
                return Ok(());
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(EVENT_LOG_PRUNE_CHUNK_GAP) => {}
            _ = signals.window_changed.notified() => return Err(Interrupt::WindowChanged),
            _ = signals.shutdown.notified() => return Err(Interrupt::Shutdown),
        }
    }
    tracing::info!(pruned_rows = pruned, ?cutoff, "event_log pruning complete");
    Ok(())
}
