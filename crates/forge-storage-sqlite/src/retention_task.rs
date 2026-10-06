use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use forge_storage::{
    ActionRepo, DEFAULT_EVENT_LOG_RETENTION_DAYS, ScheduledRunRepo, SettingsRepo,
    event_log_retention_days,
};
use time::OffsetDateTime;
use tokio::sync::Notify;

use crate::error::SqliteStorageError;
use crate::{SqliteEventLogRepo, SqliteHistoryRepo};

const MIN_EXECUTION_RETENTION_DAYS: u32 = 7;
const FIRST_SWEEP_DELAY: Duration = Duration::from_secs(30);
const PRUNE_CHUNK_ROWS: u32 = 1_000;
const PRUNE_CHUNK_GAP: Duration = Duration::from_millis(100);

pub(crate) struct RetentionSignals {
    pub(crate) window_changed: Arc<Notify>,
    pub(crate) shutdown: Arc<Notify>,
}

pub(crate) struct RetentionTargets {
    pub(crate) event_log: Arc<SqliteEventLogRepo>,
    pub(crate) history: Arc<SqliteHistoryRepo>,
    pub(crate) action: Arc<dyn ActionRepo>,
    pub(crate) settings: Arc<dyn SettingsRepo>,
    pub(crate) scheduled_run: Arc<dyn ScheduledRunRepo>,
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

    prune_in_chunks("event_log", cutoff, signals, |chunk_rows| {
        targets.event_log.prune_chunk_before(cutoff, chunk_rows)
    })
    .await?;
    prune_in_chunks("action_history", cutoff, signals, |chunk_rows| {
        targets.history.prune_chunk_before(cutoff, chunk_rows)
    })
    .await?;

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

    match targets.scheduled_run.prune_resolved_before(cutoff).await {
        Ok(pruned) => tracing::info!(
            pruned_rows = pruned,
            ?cutoff,
            "scheduled_runs pruning complete"
        ),
        Err(e) => tracing::warn!(
            error = %e,
            "scheduled_runs pruning failed; will retry on next cycle"
        ),
    }
    Ok(())
}

async fn prune_in_chunks<F, Fut>(
    table: &'static str,
    cutoff: OffsetDateTime,
    signals: &RetentionSignals,
    prune_chunk: F,
) -> Result<(), Interrupt>
where
    F: Fn(u32) -> Fut,
    Fut: Future<Output = Result<u64, SqliteStorageError>>,
{
    let mut pruned = 0u64;
    loop {
        match prune_chunk(PRUNE_CHUNK_ROWS).await {
            Ok(rows) => {
                pruned += rows;
                if rows < u64::from(PRUNE_CHUNK_ROWS) {
                    break;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, table, pruned_rows = pruned, "pruning failed; will retry on next cycle");
                return Ok(());
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(PRUNE_CHUNK_GAP) => {}
            _ = signals.window_changed.notified() => return Err(Interrupt::WindowChanged),
            _ = signals.shutdown.notified() => return Err(Interrupt::Shutdown),
        }
    }
    tracing::info!(table, pruned_rows = pruned, ?cutoff, "pruning complete");
    Ok(())
}
