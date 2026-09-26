use std::time::Duration;

use sqlx::{Sqlite, SqlitePool, pool::PoolConnection};

use crate::pool::BUSY_TIMEOUT;

const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(1);
const RESTART_AT_WAL_FRAMES: i64 = 4096;
const RESTART_READER_WAIT: Duration = Duration::from_millis(50);
const RESTART_ATTEMPTS_PER_TICK: u32 = 3;
const RESTART_RETRY_GAP: Duration = Duration::from_millis(10);
const ESCALATE_AFTER_FAILED_TICKS: u32 = 3;
const ESCALATED_READER_WAIT: Duration = Duration::from_millis(500);

/// PASSIVE copies frames off the writer connection but never resets a WAL that readers keep
/// overlapping, so past `RESTART_AT_WAL_FRAMES` the writer itself runs a bounded RESTART between writes.
pub(crate) fn spawn_checkpoint_task(checkpointer: SqlitePool, writer: SqlitePool) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(CHECKPOINT_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut restarted_at_frames = None;
        let mut failed_ticks = 0u32;
        loop {
            ticker.tick().await;
            if checkpointer.is_closed() || writer.is_closed() {
                return;
            }
            let wal_frames = match passive_checkpoint(&checkpointer).await {
                Ok(frames) => frames,
                Err(sqlx::Error::PoolClosed) => return,
                Err(e) => {
                    tracing::warn!(error = %e, "WAL checkpoint failed; retrying next tick");
                    continue;
                }
            };
            if wal_frames < RESTART_AT_WAL_FRAMES || restarted_at_frames == Some(wal_frames) {
                continue;
            }
            let escalated = failed_ticks >= ESCALATE_AFTER_FAILED_TICKS;
            match restart_within_tick(&writer, escalated).await {
                Ok(true) => {
                    if escalated {
                        tracing::info!(failed_ticks, "WAL reset after an escalated restart");
                    }
                    restarted_at_frames = Some(wal_frames);
                    failed_ticks = 0;
                }
                Ok(false) => {
                    failed_ticks = failed_ticks.saturating_add(1);
                    if failed_ticks == ESCALATE_AFTER_FAILED_TICKS {
                        tracing::warn!(
                            wal_frames,
                            failed_ticks,
                            "WAL restart keeps being outlasted by readers; waiting longer for them"
                        );
                    }
                }
                Err(sqlx::Error::PoolClosed) => return,
                Err(e) => tracing::warn!(error = %e, "WAL restart failed; retrying next tick"),
            }
        }
    });
}

async fn passive_checkpoint(checkpointer: &SqlitePool) -> Result<i64, sqlx::Error> {
    let (_busy, wal_frames, _backfilled): (i64, i64, i64) =
        sqlx::query_as("PRAGMA wal_checkpoint(PASSIVE)")
            .fetch_one(checkpointer)
            .await?;
    Ok(wal_frames)
}

async fn restart_within_tick(writer: &SqlitePool, escalated: bool) -> Result<bool, sqlx::Error> {
    if escalated {
        return restart_wal_waiting(writer, ESCALATED_READER_WAIT).await;
    }
    for attempt in 1..=RESTART_ATTEMPTS_PER_TICK {
        if restart_wal(writer).await? {
            return Ok(true);
        }
        if attempt < RESTART_ATTEMPTS_PER_TICK {
            tokio::time::sleep(RESTART_RETRY_GAP).await;
        }
    }
    Ok(false)
}

/// `Ok(false)` when a reader held an older snapshot past `RESTART_READER_WAIT`.
async fn restart_wal(writer: &SqlitePool) -> Result<bool, sqlx::Error> {
    restart_wal_waiting(writer, RESTART_READER_WAIT).await
}

async fn restart_wal_waiting(
    writer: &SqlitePool,
    reader_wait: Duration,
) -> Result<bool, sqlx::Error> {
    let mut conn = writer.acquire().await?;
    set_busy_timeout(&mut conn, reader_wait).await?;
    let restarted = sqlx::query_as::<_, (i64, i64, i64)>("PRAGMA wal_checkpoint(RESTART)")
        .fetch_one(&mut *conn)
        .await
        .map(|(busy, _, _)| busy == 0);
    if let Err(e) = set_busy_timeout(&mut conn, BUSY_TIMEOUT).await {
        conn.close_on_drop();
        return Err(e);
    }
    restarted
}

async fn set_busy_timeout(
    conn: &mut PoolConnection<Sqlite>,
    timeout: Duration,
) -> Result<(), sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "PRAGMA busy_timeout = {}",
        timeout.as_millis()
    )))
    .execute(&mut **conn)
    .await
    .map(|_| ())
}

