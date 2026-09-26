#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use forge_storage_sqlite::{SqlitePools, apply_migrations, connect_pools};
use tempfile::TempDir;

const DEADLINE: Duration = Duration::from_secs(2);

async fn file_pools() -> (SqlitePools, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("forge.db").display());
    let pools = connect_pools(&url).await.unwrap();
    apply_migrations(pools.writer()).await.unwrap();
    (pools, dir)
}

#[tokio::test]
async fn a_write_through_the_reader_pool_is_refused() {
    let (pools, _dir) = file_pools().await;

    let result = sqlx::query("INSERT INTO settings (key, value) VALUES ('k', 'v')")
        .execute(pools.reader())
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn a_read_completes_while_a_write_transaction_is_open_and_sees_only_committed_rows() {
    let (pools, _dir) = file_pools().await;
    sqlx::query("INSERT INTO settings (key, value) VALUES ('committed', 'v')")
        .execute(pools.writer())
        .await
        .unwrap();
    let mut open_write = pools.writer().begin().await.unwrap();
    sqlx::query("INSERT INTO settings (key, value) VALUES ('uncommitted', 'v')")
        .execute(&mut *open_write)
        .await
        .unwrap();

    let keys: Vec<String> = tokio::time::timeout(
        DEADLINE,
        sqlx::query_scalar("SELECT key FROM settings WHERE key IN ('committed', 'uncommitted')")
            .fetch_all(pools.reader()),
    )
    .await
    .expect("a read must not wait behind an open write transaction")
    .unwrap();

    open_write.rollback().await.unwrap();
    assert_eq!(keys, vec!["committed".to_string()]);
}

/// Frames in the WAL (`mxFrame`) and frames already copied into the database file
/// (`nBackfill`), read from the wal-index header documented at sqlite.org/walformat.html.
fn wal_progress(db: &std::path::Path) -> (u32, u32) {
    use std::io::Read;

    let mut shm = std::fs::File::open(db.with_extension("db-shm")).unwrap();
    let mut header = [0_u8; 100];
    shm.read_exact(&mut header).unwrap();
    let field = |at: usize| u32::from_ne_bytes(header[at..at + 4].try_into().unwrap());
    (field(16), field(96))
}

#[tokio::test]
async fn the_backend_checkpoints_what_the_writer_left_in_the_wal() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("forge.db");
    let _backend =
        common::sandboxed_backend(&format!("sqlite://{}", db.display()), common::TEST_KEY).await;

    let mut progress = wal_progress(&db);
    for _ in 0..250 {
        if progress.0 > 0 && progress.1 == progress.0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        progress = wal_progress(&db);
    }
    assert!(
        progress.0 > 0 && progress.1 == progress.0,
        "WAL frames {} / backfilled {}",
        progress.0,
        progress.1
    );
}
