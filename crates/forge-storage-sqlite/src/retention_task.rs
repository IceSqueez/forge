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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::{BTreeMap, VecDeque};

    use forge_storage::HistoryRepo;
    use forge_types::{ActionId, EventId, ExecutionContext, ExecutionMetadata, ExecutionOutcome};

    use super::*;
    use crate::{apply_migrations, connect};

    const FULL: u64 = PRUNE_CHUNK_ROWS as u64;

    fn signals() -> RetentionSignals {
        RetentionSignals {
            window_changed: Arc::new(Notify::new()),
            shutdown: Arc::new(Notify::new()),
        }
    }

    fn cutoff() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
    }

    struct ScriptedChunks {
        replies: RefCell<VecDeque<Result<u64, SqliteStorageError>>>,
        calls: Cell<usize>,
    }

    impl ScriptedChunks {
        fn new(replies: impl IntoIterator<Item = Result<u64, SqliteStorageError>>) -> Self {
            Self {
                replies: RefCell::new(replies.into_iter().collect()),
                calls: Cell::new(0),
            }
        }

        fn next(&self) -> impl Future<Output = Result<u64, SqliteStorageError>> + use<> {
            self.calls.set(self.calls.get() + 1);
            let reply = self.replies.borrow_mut().pop_front().unwrap_or(Ok(0));
            async move { reply }
        }
    }

    async fn run(script: &ScriptedChunks, signals: &RetentionSignals) -> Result<(), Interrupt> {
        prune_in_chunks("action_history", cutoff(), signals, |_| script.next()).await
    }

    #[tokio::test(start_paused = true)]
    async fn keeps_pruning_while_chunks_come_back_full_and_stops_on_the_first_short_one() {
        for (replies, expected_calls) in [
            (vec![0], 1),
            (vec![FULL - 1], 1),
            (vec![FULL, FULL - 1], 2),
            (vec![FULL, FULL, 0], 3),
            (vec![FULL, FULL, FULL, 1], 4),
        ] {
            let script = ScriptedChunks::new(replies.iter().copied().map(Ok));

            let outcome = run(&script, &signals()).await;

            assert!(outcome.is_ok(), "{replies:?}");
            assert_eq!(script.calls.get(), expected_calls, "{replies:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn waits_the_chunk_gap_between_consecutive_chunks() {
        let script = ScriptedChunks::new([Ok(FULL), Ok(FULL), Ok(0)]);
        let started = tokio::time::Instant::now();

        run(&script, &signals()).await.ok();

        assert_eq!(started.elapsed(), PRUNE_CHUNK_GAP * 2);
    }

    #[tokio::test(start_paused = true)]
    async fn a_pending_signal_stops_pruning_after_the_current_chunk() {
        for shutdown in [false, true] {
            let signals = signals();
            if shutdown {
                signals.shutdown.notify_one();
            } else {
                signals.window_changed.notify_one();
            }
            let script = ScriptedChunks::new([Ok(FULL), Ok(FULL), Ok(FULL), Ok(0)]);

            let outcome = run(&script, &signals).await;

            if shutdown {
                assert!(matches!(outcome, Err(Interrupt::Shutdown)));
            } else {
                assert!(matches!(outcome, Err(Interrupt::WindowChanged)));
            }
            assert_eq!(script.calls.get(), 1, "shutdown={shutdown}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_chunk_ends_this_cycle_quietly_without_further_chunks() {
        for (full_chunks_before_failure, expected_calls) in [(0, 1), (2, 3)] {
            let mut replies: Vec<_> = (0..full_chunks_before_failure).map(|_| Ok(FULL)).collect();
            replies.push(Err(SqliteStorageError::Decode("disk I/O error".into())));
            replies.push(Ok(FULL));
            let script = ScriptedChunks::new(replies);

            let outcome = run(&script, &signals()).await;

            assert!(outcome.is_ok());
            assert_eq!(script.calls.get(), expected_calls);
        }
    }

    fn run_started_at(started_at: OffsetDateTime) -> ExecutionContext {
        ExecutionContext {
            action_id: ActionId::new(),
            metadata: ExecutionMetadata::Trigger {
                event_id: EventId::new(),
                trigger_kind: None,
            },
            arg_stack_snapshot: BTreeMap::new(),
            started_at,
            completed_at: Some(started_at),
            telemetry: vec![],
            outcome: ExecutionOutcome::Success,
        }
    }

    #[test]
    fn chunked_prune_of_action_history_removes_every_older_run_across_several_chunks() {
        let db_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let paused_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap();
        let older_runs = 2 * i64::from(PRUNE_CHUNK_ROWS) + 500;
        let newer = run_started_at(cutoff());
        let mut runs: Vec<_> = (1..=older_runs)
            .map(|ms| run_started_at(cutoff() - time::Duration::milliseconds(ms)))
            .collect();
        runs.push(newer.clone());
        let history = db_runtime.block_on(async {
            let pool = connect(":memory:").await.unwrap();
            apply_migrations(&pool).await.unwrap();
            let history = Arc::new(SqliteHistoryRepo::new(pool));
            history.save_batch(&runs).await.unwrap();
            history
        });

        let outcome = paused_runtime.block_on(prune_in_chunks(
            "action_history",
            cutoff(),
            &signals(),
            |rows| {
                let history = Arc::clone(&history);
                let chunk = db_runtime
                    .spawn(async move { history.prune_chunk_before(cutoff(), rows).await });
                async move { chunk.await.unwrap() }
            },
        ));

        assert!(outcome.is_ok());
        let survivors = db_runtime.block_on(history.recent(u32::MAX)).unwrap();
        assert_eq!(
            survivors
                .iter()
                .map(|run| run.started_at)
                .collect::<Vec<_>>(),
            vec![newer.started_at]
        );
    }
}
