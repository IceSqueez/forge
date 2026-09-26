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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use tempfile::TempDir;
    use tokio::time::Instant;

    use super::*;
    use crate::pool::{SqlitePools, connect_pools};

    const RESET_DEADLINE: Duration = Duration::from_secs(8);
    const POLL: Duration = Duration::from_millis(20);
    const SMALL_PAGE_BYTES: u32 = 512;
    const JOURNAL_SIZE_LIMIT: u64 = 64 * 1024 * 1024;

    struct Db {
        pools: SqlitePools,
        path: PathBuf,
        _dir: TempDir,
    }

    async fn db_with_page_size(page_size: u32) -> Db {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forge.db");
        let url = format!("sqlite://{}", path.display());
        let seed = sqlx::SqlitePool::connect(&format!("{url}?mode=rwc"))
            .await
            .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "PRAGMA page_size = {page_size}"
        )))
        .execute(&seed)
        .await
        .unwrap();
        sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY, body BLOB)")
            .execute(&seed)
            .await
            .unwrap();
        seed.close().await;
        let pools = connect_pools(&url).await.unwrap();
        Db {
            pools,
            path,
            _dir: dir,
        }
    }

    async fn insert(writer: &SqlitePool) {
        sqlx::query("INSERT INTO t (body) VALUES (x'00')")
            .execute(writer)
            .await
            .unwrap();
    }

    /// `mxFrame` of the wal-index header (sqlite.org/walformat.html); it only ever falls when
    /// the WAL is reset to its start.
    fn wal_frames(db: &Path) -> u32 {
        use std::io::Read;

        let mut shm = std::fs::File::open(db.with_extension("db-shm")).unwrap();
        let mut header = [0_u8; 20];
        shm.read_exact(&mut header).unwrap();
        u32::from_ne_bytes(header[16..20].try_into().unwrap())
    }

    fn wal_bytes(db: &Path) -> u64 {
        std::fs::metadata(db.with_extension("db-wal"))
            .unwrap()
            .len()
    }

    async fn writer_busy_timeout_ms(writer: &SqlitePool) -> i64 {
        sqlx::query_scalar("PRAGMA busy_timeout")
            .fetch_one(writer)
            .await
            .unwrap()
    }

    async fn pin_old_snapshot(db: &Db) -> PoolConnection<Sqlite> {
        let mut reader = db.pools.reader().acquire().await.unwrap();
        sqlx::query("BEGIN").execute(&mut *reader).await.unwrap();
        let _: i64 = sqlx::query_scalar("SELECT count(*) FROM t")
            .fetch_one(&mut *reader)
            .await
            .unwrap();
        insert(db.pools.writer()).await;
        reader
    }

    #[tokio::test]
    async fn a_restart_held_up_by_a_reader_gives_up_after_the_short_reader_wait() {
        let db = db_with_page_size(4096).await;
        insert(db.pools.writer()).await;
        let _reader = pin_old_snapshot(&db).await;

        let started = Instant::now();
        let restarted = restart_wal(db.pools.writer()).await.unwrap();

        assert_eq!(
            (restarted, started.elapsed() < Duration::from_secs(1)),
            (false, true),
            "took {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn the_writer_gets_its_full_busy_timeout_back_after_a_restart_either_way() {
        for reader_in_the_way in [false, true] {
            let db = db_with_page_size(4096).await;
            insert(db.pools.writer()).await;
            let reader = if reader_in_the_way {
                Some(pin_old_snapshot(&db).await)
            } else {
                None
            };

            let restarted = restart_wal(db.pools.writer()).await.unwrap();
            drop(reader);

            assert_eq!(
                (restarted, writer_busy_timeout_ms(db.pools.writer()).await),
                (!reader_in_the_way, BUSY_TIMEOUT.as_millis() as i64),
                "reader in the way: {reader_in_the_way}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_wal_that_an_unthrottled_writer_and_overlapping_readers_keep_busy_is_reset() {
        let db = db_with_page_size(SMALL_PAGE_BYTES).await;
        let running = Arc::new(AtomicBool::new(true));
        let mut load = Vec::new();
        {
            let writer = db.pools.writer().clone();
            let running = Arc::clone(&running);
            load.push(tokio::spawn(async move {
                while running.load(Ordering::Relaxed) {
                    insert(&writer).await;
                }
            }));
        }
        for _ in 0..2 {
            let reader = db.pools.reader().clone();
            let running = Arc::clone(&running);
            load.push(tokio::spawn(async move {
                while running.load(Ordering::Relaxed) {
                    let mut tx = reader.begin().await.unwrap();
                    let _: i64 = sqlx::query_scalar("SELECT count(*) FROM t")
                        .fetch_one(&mut *tx)
                        .await
                        .unwrap();
                    tx.rollback().await.unwrap();
                }
            }));
        }
        spawn_checkpoint_task(
            db.pools.checkpointer().unwrap().clone(),
            db.pools.writer().clone(),
        );

        let deadline = Instant::now() + RESET_DEADLINE;
        let mut peak = wal_frames(&db.path);
        let mut reset_from = None;
        while Instant::now() < deadline {
            tokio::time::sleep(POLL).await;
            let frames = wal_frames(&db.path);
            if frames < peak {
                reset_from = Some(peak);
                break;
            }
            peak = frames;
        }
        running.store(false, Ordering::Relaxed);
        for task in load {
            task.await.unwrap();
        }

        let reset_past_threshold =
            reset_from.is_some_and(|frames| i64::from(frames) >= RESTART_AT_WAL_FRAMES);
        assert!(
            reset_past_threshold,
            "WAL never reset after passing {RESTART_AT_WAL_FRAMES} frames (reset from {reset_from:?}, peak {peak})"
        );
    }

    #[tokio::test]
    async fn a_wal_grown_past_the_size_limit_is_cut_back_to_it_after_the_reset() {
        let db = db_with_page_size(4096).await;
        let over_limit = JOURNAL_SIZE_LIMIT + 8 * 1024 * 1024;
        sqlx::query("INSERT INTO t (body) VALUES (zeroblob(?))")
            .bind(over_limit as i64)
            .execute(db.pools.writer())
            .await
            .unwrap();
        assert!(wal_bytes(&db.path) > over_limit);
        spawn_checkpoint_task(
            db.pools.checkpointer().unwrap().clone(),
            db.pools.writer().clone(),
        );

        let deadline = Instant::now() + RESET_DEADLINE;
        let mut size = wal_bytes(&db.path);
        while size > JOURNAL_SIZE_LIMIT && Instant::now() < deadline {
            tokio::time::sleep(POLL).await;
            insert(db.pools.writer()).await;
            size = wal_bytes(&db.path);
        }

        assert!(
            size <= JOURNAL_SIZE_LIMIT,
            "WAL file still {size} bytes, limit {JOURNAL_SIZE_LIMIT}"
        );
    }
}
