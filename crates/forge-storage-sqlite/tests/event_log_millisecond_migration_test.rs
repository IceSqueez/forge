#![expect(clippy::expect_used, reason = "test fixtures")]
mod common;
use forge_storage::EventLogRepo;
use forge_storage_sqlite::{
    MIGRATIONS, SqliteEventLogRepo, SqlitePools, apply_migrations, connect_pools,
};
use tempfile::TempDir;

const VERSION_BEFORE_MILLISECONDS: i64 = 60;
const STORED_SECONDS: i64 = 1_700_000_000;

async fn migrated_from_second_resolution() -> (SqlitePools, TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = common::database_url(&dir);
    let pools = connect_pools(&url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_MILLISECONDS, pools.writer())
        .await
        .expect("the schema before the change");
    for (id, seconds) in [
        ("01ARZ3NDEKTSV4RRFFQ69G5FAV", STORED_SECONDS),
        ("01ARZ3NDEKTSV4RRFFQ69G5FAW", STORED_SECONDS + 1),
    ] {
        sqlx::query(
            "INSERT INTO event_log (id, source, kind, timestamp, payload, replay)
             VALUES (?, 'core', 'timer.tick', ?, 'null', 0)",
        )
        .bind(id)
        .bind(seconds)
        .execute(pools.writer())
        .await
        .expect("an event row in seconds");
    }
    apply_migrations(pools.writer()).await.expect("migrate");
    (pools, dir)
}

#[tokio::test]
async fn migration_scales_every_stored_timestamp_from_seconds_to_milliseconds() {
    let (pools, _dir) = migrated_from_second_resolution().await;

    let stored: Vec<i64> = sqlx::query_scalar("SELECT timestamp FROM event_log ORDER BY timestamp")
        .fetch_all(pools.writer())
        .await
        .expect("timestamps");

    assert_eq!(
        stored,
        vec![
            STORED_SECONDS * common::MILLIS_PER_SECOND,
            (STORED_SECONDS + 1) * common::MILLIS_PER_SECOND
        ]
    );
}

#[tokio::test]
async fn migrated_rows_decode_to_the_original_instants() {
    let (pools, _dir) = migrated_from_second_resolution().await;

    let events = SqliteEventLogRepo::new(pools)
        .recent_since(10, None)
        .await
        .expect("recent");

    let seconds: Vec<i64> = events
        .iter()
        .map(|e| e.timestamp.unix_timestamp())
        .collect();
    assert_eq!(seconds, vec![STORED_SECONDS + 1, STORED_SECONDS]);
}
