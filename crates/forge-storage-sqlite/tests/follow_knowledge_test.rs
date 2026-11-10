#![expect(clippy::expect_used, reason = "test fixtures")]
use forge_storage::{FollowKnowledgeEntry, FollowKnowledgeKey, FollowKnowledgeRepo, KnownFollow};
use forge_storage_sqlite::{SqliteFollowKnowledgeRepo, SqlitePools};
use forge_types::PlatformId;
use time::{Duration, OffsetDateTime};

mod common;

const POISONED_VIEWER: &str = "poisoned";
const ONE_STATEMENT_OF_ROWS: usize = 256;
const ROWS_SPANNING_THREE_DELETE_CHUNKS: usize = 1_001;

struct Store {
    repo: SqliteFollowKnowledgeRepo,
    pools: common::Sandboxed<SqlitePools>,
}

async fn store() -> Store {
    let pools = common::migrated_pools().await;
    Store {
        repo: SqliteFollowKnowledgeRepo::new(pools.clone()),
        pools,
    }
}

fn at(millis: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_790_000_000).expect("a valid instant")
        + Duration::milliseconds(millis)
}

fn key(platform: PlatformId, viewer_id: &str) -> FollowKnowledgeKey {
    FollowKnowledgeKey {
        platform,
        viewer_id: viewer_id.to_owned(),
    }
}

fn entry(
    key: FollowKnowledgeKey,
    follow: KnownFollow,
    observed_at: OffsetDateTime,
) -> FollowKnowledgeEntry {
    FollowKnowledgeEntry {
        key,
        follow,
        observed_at,
        fresh_until: observed_at + Duration::hours(1),
    }
}

fn expiring(key: FollowKnowledgeKey, fresh_until: OffsetDateTime) -> FollowKnowledgeEntry {
    FollowKnowledgeEntry {
        key,
        follow: KnownFollow::NotFollowing,
        observed_at: at(0),
        fresh_until,
    }
}

fn twitch(viewer_id: &str) -> FollowKnowledgeKey {
    key(PlatformId::Twitch, viewer_id)
}

async fn stored(store: &Store, key: &FollowKnowledgeKey) -> Option<FollowKnowledgeEntry> {
    store.repo.get(key).await.expect("a follow knowledge read")
}

async fn row_count(store: &Store) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM follow_knowledge")
        .fetch_one(store.pools.reader())
        .await
        .expect("a row count")
}

async fn reject_inserts_of_the_poisoned_viewer(store: &Store) {
    sqlx::query(
        "CREATE TRIGGER reject_poisoned BEFORE INSERT ON follow_knowledge \
         WHEN NEW.viewer_id = 'poisoned' BEGIN SELECT RAISE(ABORT, 'poisoned row'); END",
    )
    .execute(store.pools.writer())
    .await
    .expect("a rejecting trigger");
}

#[tokio::test]
async fn upsert_then_get_round_trips_every_known_follow_at_millisecond_precision() {
    let store = store().await;
    let cases = [
        ("follower", KnownFollow::FollowingSince(at(-86_400_123))),
        ("stranger", KnownFollow::NotFollowing),
        ("private", KnownFollow::Hidden),
    ];

    for (viewer, follow) in cases {
        let written = FollowKnowledgeEntry {
            key: key(PlatformId::YouTube, viewer),
            follow,
            observed_at: at(457),
            fresh_until: at(3_600_789),
        };
        store.repo.upsert(&written).await.expect("an upsert");

        assert_eq!(
            stored(&store, &written.key).await,
            Some(written),
            "{viewer}"
        );
    }
}

#[tokio::test]
async fn get_of_an_unknown_viewer_is_none() {
    let store = store().await;
    store
        .repo
        .upsert(&entry(twitch("known"), KnownFollow::Hidden, at(0)))
        .await
        .expect("an upsert");

    assert_eq!(stored(&store, &twitch("unknown")).await, None);
    assert_eq!(stored(&store, &key(PlatformId::Kick, "known")).await, None);
}

#[tokio::test]
async fn upsert_replaces_a_newer_row_with_an_older_observation() {
    let store = store().await;
    let newer = entry(twitch("viewer"), KnownFollow::Hidden, at(5_000));
    let older = entry(
        twitch("viewer"),
        KnownFollow::FollowingSince(at(-1_000)),
        at(1_000),
    );
    store.repo.upsert(&newer).await.expect("the newer upsert");
    store.repo.upsert(&older).await.expect("the older upsert");

    assert_eq!(stored(&store, &older.key).await, Some(older));
}

#[tokio::test]
async fn get_returns_a_row_whose_freshness_has_expired() {
    let store = store().await;
    let expired = expiring(twitch("viewer"), at(-60_000));
    store.repo.upsert(&expired).await.expect("an upsert");

    assert_eq!(stored(&store, &expired.key).await, Some(expired));
}

#[tokio::test]
async fn fresh_database_is_not_seeded() {
    let store = store().await;

    assert!(!store.repo.is_seeded().await.expect("the seeded marker"));
}

#[tokio::test]
async fn seed_with_entries_stores_them_and_marks_seeded() {
    let store = store().await;
    let seeded = entry(twitch("viewer"), KnownFollow::NotFollowing, at(0));
    store
        .repo
        .seed(std::slice::from_ref(&seeded))
        .await
        .expect("a seed");

    assert!(store.repo.is_seeded().await.expect("the seeded marker"));
    assert_eq!(stored(&store, &seeded.key).await, Some(seeded));
}

#[tokio::test]
async fn seed_with_no_entries_still_marks_seeded() {
    let store = store().await;
    store.repo.seed(&[]).await.expect("an empty seed");

    assert!(store.repo.is_seeded().await.expect("the seeded marker"));
}

#[tokio::test]
async fn seed_failing_in_a_later_statement_leaves_no_rows_and_no_marker() {
    let store = store().await;
    reject_inserts_of_the_poisoned_viewer(&store).await;
    let mut entries: Vec<FollowKnowledgeEntry> = (0..=ONE_STATEMENT_OF_ROWS)
        .map(|n| entry(twitch(&format!("viewer-{n}")), KnownFollow::Hidden, at(0)))
        .collect();
    entries.push(entry(twitch(POISONED_VIEWER), KnownFollow::Hidden, at(0)));

    assert!(store.repo.seed(&entries).await.is_err());
    assert_eq!(row_count(&store).await, 0);
    assert!(!store.repo.is_seeded().await.expect("the seeded marker"));
}

#[tokio::test]
async fn seed_keeps_an_existing_row_against_an_older_observation() {
    let store = store().await;
    let existing = entry(twitch("viewer"), KnownFollow::Hidden, at(5_000));
    store.repo.upsert(&existing).await.expect("an upsert");
    store
        .repo
        .seed(&[entry(
            twitch("viewer"),
            KnownFollow::NotFollowing,
            at(1_000),
        )])
        .await
        .expect("a seed");

    assert_eq!(stored(&store, &existing.key).await, Some(existing));
}

#[tokio::test]
async fn seed_replaces_an_existing_row_with_a_newer_observation() {
    let store = store().await;
    store
        .repo
        .upsert(&entry(twitch("viewer"), KnownFollow::Hidden, at(1_000)))
        .await
        .expect("an upsert");
    let newer = entry(
        twitch("viewer"),
        KnownFollow::FollowingSince(at(-1)),
        at(1_001),
    );
    store
        .repo
        .seed(std::slice::from_ref(&newer))
        .await
        .expect("a seed");

    assert_eq!(stored(&store, &newer.key).await, Some(newer));
}

#[tokio::test]
async fn seed_keeps_an_existing_row_against_an_equally_old_observation() {
    let store = store().await;
    let existing = entry(twitch("viewer"), KnownFollow::Hidden, at(1_000));
    store.repo.upsert(&existing).await.expect("an upsert");
    store
        .repo
        .seed(&[entry(
            twitch("viewer"),
            KnownFollow::NotFollowing,
            at(1_000),
        )])
        .await
        .expect("a seed");

    assert_eq!(stored(&store, &existing.key).await, Some(existing));
}

#[tokio::test]
async fn seed_duplicates_in_one_batch_keep_the_newest_in_either_order() {
    let older = entry(twitch("viewer"), KnownFollow::NotFollowing, at(1_000));
    let newer = entry(
        twitch("viewer"),
        KnownFollow::FollowingSince(at(-1)),
        at(2_000),
    );

    for batch in [
        [older.clone(), newer.clone()],
        [newer.clone(), older.clone()],
    ] {
        let store = store().await;
        store.repo.seed(&batch).await.expect("a seed");

        assert_eq!(stored(&store, &newer.key).await, Some(newer.clone()));
    }
}

#[tokio::test]
async fn prune_expired_removes_rows_at_or_before_now_and_keeps_later_ones() {
    let store = store().await;
    let now = at(10_000);
    let later = expiring(twitch("later"), now + Duration::milliseconds(1));
    store
        .repo
        .seed(&[
            expiring(twitch("earlier"), now - Duration::milliseconds(1)),
            expiring(twitch("exactly-now"), now),
            later.clone(),
        ])
        .await
        .expect("a seed");

    assert_eq!(store.repo.prune_expired(now).await.expect("a prune"), 2);
    assert_eq!(row_count(&store).await, 1);
    assert_eq!(stored(&store, &later.key).await, Some(later));
}

#[tokio::test]
async fn prune_expired_counts_every_row_removed_across_delete_chunks() {
    let store = store().await;
    let now = at(10_000);
    let mut entries: Vec<FollowKnowledgeEntry> = (0..ROWS_SPANNING_THREE_DELETE_CHUNKS)
        .map(|n| expiring(twitch(&format!("expired-{n}")), at(0)))
        .collect();
    entries.push(expiring(twitch("fresh"), at(20_000)));
    store.repo.seed(&entries).await.expect("a seed");

    assert_eq!(
        store.repo.prune_expired(now).await.expect("a prune"),
        u64::try_from(ROWS_SPANNING_THREE_DELETE_CHUNKS).expect("a row count")
    );
    assert_eq!(row_count(&store).await, 1);
}

#[tokio::test]
async fn clear_platform_removes_only_that_platform_and_keeps_the_seeded_marker() {
    let store = store().await;
    let kick = entry(key(PlatformId::Kick, "viewer"), KnownFollow::Hidden, at(0));
    store
        .repo
        .seed(&[
            entry(twitch("viewer"), KnownFollow::Hidden, at(0)),
            entry(twitch("other"), KnownFollow::NotFollowing, at(0)),
            kick.clone(),
        ])
        .await
        .expect("a seed");

    assert_eq!(
        store
            .repo
            .clear_platform(PlatformId::Twitch)
            .await
            .expect("a clear"),
        2
    );
    assert_eq!(row_count(&store).await, 1);
    assert_eq!(stored(&store, &kick.key).await, Some(kick));
    assert!(store.repo.is_seeded().await.expect("the seeded marker"));
}
