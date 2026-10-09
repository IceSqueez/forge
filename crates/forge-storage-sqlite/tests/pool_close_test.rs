#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_storage_sqlite::{SqlitePools, apply_migrations, connect_pools};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, SqliteConnection, SqlitePool};
use tempfile::TempDir;

const DEADLINE: Duration = Duration::from_secs(30);
const UNDER_THE_BUSY_TIMEOUT: Duration = Duration::from_millis(4_500);
const ROWS: i64 = 50;

fn wal_of(db: &Path) -> PathBuf {
    db.with_extension("db-wal")
}

fn wal_len(db: &Path) -> u64 {
    std::fs::metadata(wal_of(db)).map_or(0, |m| m.len())
}

async fn populated() -> (SqlitePools, PathBuf, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("forge.db");
    let pools = connect_pools(&format!("sqlite://{}", db.display()))
        .await
        .unwrap();
    apply_migrations(pools.writer()).await.unwrap();
    for i in 0..ROWS {
        sqlx::query("INSERT INTO settings (key, value) VALUES (?, 'v')")
            .bind(format!("k{i}"))
            .execute(pools.writer())
            .await
            .unwrap();
    }
    (pools, db, dir)
}

async fn row_count(db: &Path) -> i64 {
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(db))
        .await
        .unwrap();
    let count = sqlx::query_scalar("SELECT COUNT(*) FROM settings WHERE key LIKE 'k%'")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    count
}

#[tokio::test]
async fn close_leaves_no_wal_content_and_every_row_survives_reopen() {
    let (pools, db, _dir) = populated().await;
    assert!(wal_len(&db) > 0, "fixture must leave frames in the WAL");

    pools.close().await;

    assert_eq!(wal_len(&db), 0);
    assert_eq!(row_count(&db).await, ROWS);
}

#[tokio::test]
async fn close_returns_in_bounded_time_while_a_reader_holds_a_snapshot_and_data_stays_consistent() {
    let (pools, db, _dir) = populated().await;
    let mut foreign = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&db))
        .await
        .unwrap();
    let mut snapshot = foreign.begin().await.unwrap();
    let _: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings")
        .fetch_one(&mut *snapshot)
        .await
        .unwrap();
    sqlx::query("INSERT INTO settings (key, value) VALUES ('k_after', 'v')")
        .execute(pools.writer())
        .await
        .unwrap();

    tokio::time::timeout(UNDER_THE_BUSY_TIMEOUT, pools.close())
        .await
        .expect("close must not wait for a reader's snapshot");

    snapshot.rollback().await.unwrap();
    foreign.close().await.unwrap();
    assert_eq!(row_count(&db).await, ROWS + 1);
}

#[tokio::test]
async fn closing_a_single_pool_without_a_checkpointer_does_not_hang() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let pools = SqlitePools::from(pool);

    tokio::time::timeout(DEADLINE, pools.close())
        .await
        .expect("in-memory close must finish");
}

#[tokio::test]
async fn closing_twice_is_harmless() {
    let (pools, db, _dir) = populated().await;

    pools.close().await;
    tokio::time::timeout(DEADLINE, pools.close())
        .await
        .expect("second close must finish");

    assert_eq!(row_count(&db).await, ROWS);
}

const CYCLES: usize = 40;

fn shm_of(db: &Path) -> PathBuf {
    db.with_extension("db-shm")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_removes_wal_and_shm_files_on_every_open_write_close_cycle() {
    let mut leftovers = Vec::new();
    for cycle in 0..CYCLES {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("forge.db");
        let pools = connect_pools(&format!("sqlite://{}", db.display()))
            .await
            .unwrap();
        apply_migrations(pools.writer()).await.unwrap();
        sqlx::query("INSERT INTO settings (key, value) VALUES ('k', 'v')")
            .execute(pools.writer())
            .await
            .unwrap();
        let _: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings")
            .fetch_one(pools.reader())
            .await
            .unwrap();

        pools.close().await;

        if wal_of(&db).exists() || shm_of(&db).exists() {
            leftovers.push(cycle);
        }
    }
    assert!(
        leftovers.is_empty(),
        "wal/shm files survived close in cycles {leftovers:?}"
    );
}
