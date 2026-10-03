#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use common::{Sandboxed, TEST_KEY, sandboxed_backend};
use forge_storage::{DataProvider, LatestRecord, LatestValueRepo};
use forge_storage_sqlite::SqliteBackend;
use serde_json::json;
use time::OffsetDateTime;

const SLOT: &str = "latest_donation";
const OTHER_SLOT: &str = "latest_follower";

async fn backend() -> Sandboxed<SqliteBackend> {
    sandboxed_backend(":memory:", TEST_KEY).await
}

fn repo(backend: &SqliteBackend) -> Arc<dyn LatestValueRepo> {
    backend.latest_value_repo()
}

fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_790_000_000 + seconds).expect("a valid instant")
}

fn record(
    slot: &str,
    platform: &str,
    occurred: i64,
    updated: i64,
    payload: serde_json::Value,
) -> LatestRecord {
    LatestRecord {
        slot: slot.to_owned(),
        platform: platform.to_owned(),
        payload,
        occurred_at: at(occurred),
        updated_at: at(updated),
    }
}

#[tokio::test]
async fn upsert_into_empty_key_inserts_and_returns_true() {
    let backend = backend().await;
    let repo = repo(&backend);
    let first = record(SLOT, "twitch", 10, 10, json!({"who": "a"}));

    assert!(repo.upsert_if_newer(&first).await.unwrap());
    assert_eq!(repo.list_slot(SLOT).await.unwrap(), vec![first]);
}

#[tokio::test]
async fn upsert_with_newer_occurred_at_overwrites_payload_and_timestamps() {
    let backend = backend().await;
    let repo = repo(&backend);
    repo.upsert_if_newer(&record(SLOT, "twitch", 10, 10, json!({"who": "old"})))
        .await
        .unwrap();
    let newer = record(SLOT, "twitch", 11, 50, json!({"who": "new"}));

    assert!(repo.upsert_if_newer(&newer).await.unwrap());
    assert_eq!(repo.list_slot(SLOT).await.unwrap(), vec![newer]);
}

#[tokio::test]
async fn upsert_with_equal_or_older_occurred_at_returns_false_and_keeps_row_intact() {
    let backend = backend().await;
    let repo = repo(&backend);
    let stored = record(SLOT, "twitch", 10, 10, json!({"who": "kept"}));
    repo.upsert_if_newer(&stored).await.unwrap();

    for occurred in [10, 9] {
        let stale = record(SLOT, "twitch", occurred, 99, json!({"who": "stale"}));
        assert!(
            !repo.upsert_if_newer(&stale).await.unwrap(),
            "occurred {occurred}"
        );
        assert_eq!(repo.list_slot(SLOT).await.unwrap(), vec![stored.clone()]);
    }
}

#[tokio::test]
async fn platforms_in_one_slot_keep_separate_rows() {
    let backend = backend().await;
    let repo = repo(&backend);
    let twitch = record(SLOT, "twitch", 20, 20, json!("t"));
    let kick = record(SLOT, "kick", 10, 10, json!("k"));

    assert!(repo.upsert_if_newer(&twitch).await.unwrap());
    assert!(repo.upsert_if_newer(&kick).await.unwrap());
    assert_eq!(repo.list_slot(SLOT).await.unwrap(), vec![twitch, kick]);
}

#[tokio::test]
async fn list_slot_orders_by_occurred_at_descending() {
    let backend = backend().await;
    let repo = repo(&backend);
    let oldest = record(SLOT, "a", 1, 1, json!(1));
    let newest = record(SLOT, "b", 30, 1, json!(2));
    let middle = record(SLOT, "c", 15, 1, json!(3));
    for r in [&oldest, &newest, &middle] {
        repo.upsert_if_newer(r).await.unwrap();
    }

    assert_eq!(
        repo.list_slot(SLOT).await.unwrap(),
        vec![newest, middle, oldest]
    );
}

#[tokio::test]
async fn list_slot_returns_only_its_own_slot_and_empty_for_unknown() {
    let backend = backend().await;
    let repo = repo(&backend);
    let mine = record(SLOT, "twitch", 1, 1, json!(1));
    repo.upsert_if_newer(&mine).await.unwrap();
    repo.upsert_if_newer(&record(OTHER_SLOT, "twitch", 2, 2, json!(2)))
        .await
        .unwrap();

    assert_eq!(repo.list_slot(SLOT).await.unwrap(), vec![mine]);
    assert!(repo.list_slot("never_written").await.unwrap().is_empty());
}

#[tokio::test]
async fn payload_round_trips_nested_json() {
    let backend = backend().await;
    let repo = repo(&backend);
    let nested = record(
        SLOT,
        "twitch",
        1,
        1,
        json!({"amount": 12.5, "tags": ["a", null, {"k": true}], "name": "Іван \"q\""}),
    );
    repo.upsert_if_newer(&nested).await.unwrap();

    assert_eq!(
        repo.list_slot(SLOT).await.unwrap()[0].payload,
        nested.payload
    );
}

#[tokio::test]
async fn reset_slot_deletes_every_platform_of_that_slot_only() {
    let backend = backend().await;
    let repo = repo(&backend);
    repo.upsert_if_newer(&record(SLOT, "twitch", 1, 1, json!(1)))
        .await
        .unwrap();
    repo.upsert_if_newer(&record(SLOT, "kick", 2, 2, json!(2)))
        .await
        .unwrap();
    let survivor = record(OTHER_SLOT, "twitch", 3, 3, json!(3));
    repo.upsert_if_newer(&survivor).await.unwrap();

    assert_eq!(repo.reset_slot(SLOT).await.unwrap(), 2);
    assert!(repo.list_slot(SLOT).await.unwrap().is_empty());
    assert_eq!(repo.list_slot(OTHER_SLOT).await.unwrap(), vec![survivor]);
    assert_eq!(repo.reset_slot(SLOT).await.unwrap(), 0);
}

#[tokio::test]
async fn older_record_is_accepted_again_after_reset() {
    let backend = backend().await;
    let repo = repo(&backend);
    repo.upsert_if_newer(&record(SLOT, "twitch", 100, 100, json!("big")))
        .await
        .unwrap();
    repo.reset_slot(SLOT).await.unwrap();
    let older = record(SLOT, "twitch", 5, 5, json!("small"));

    assert!(repo.upsert_if_newer(&older).await.unwrap());
    assert_eq!(repo.list_slot(SLOT).await.unwrap(), vec![older]);
}
