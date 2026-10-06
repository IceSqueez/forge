#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::{
    BAN_LEDGER_LIST_CAP, BanLedgerEntry, BanLedgerKey, BanLedgerRepo, BanOrigin, ViewerPlatform,
};
use forge_storage_sqlite::{MIGRATIONS, SqliteBanLedgerRepo, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;
use time::{Duration, OffsetDateTime};

const VERSION_BEFORE_BAN_LEDGER: i64 = 51;
const CHANNEL: &str = "channel";
const TEN_MINUTES: Duration = Duration::minutes(10);

struct Ledger {
    repo: SqliteBanLedgerRepo,
    pool: SqlitePool,
    _dir: TempDir,
}

async fn empty_pool() -> (SqlitePool, TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}", dir.path().join("forge.db").display());
    let pool = connect(&url).await.expect("a database");
    (pool, dir)
}

async fn ledger() -> Ledger {
    let (pool, dir) = empty_pool().await;
    apply_migrations(&pool).await.expect("migrate");
    Ledger {
        repo: SqliteBanLedgerRepo::new(pool.clone()),
        pool,
        _dir: dir,
    }
}

fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_790_000_000 + seconds).expect("a valid instant")
}

fn key(platform: ViewerPlatform, channel_id: &str, viewer_id: &str) -> BanLedgerKey {
    BanLedgerKey {
        platform,
        channel_id: channel_id.to_owned(),
        viewer_id: viewer_id.to_owned(),
    }
}

fn ban(key: BanLedgerKey, banned_at: OffsetDateTime, term: Option<Duration>) -> BanLedgerEntry {
    BanLedgerEntry {
        viewer_name: format!("name_of_{}", key.viewer_id),
        key,
        reason: Some("spam".to_owned()),
        moderator: Some("mod".to_owned()),
        banned_at,
        expires_at: term.map(|term| banned_at + term),
        platform_ban_id: None,
        origin: BanOrigin::IssuedByForge,
    }
}

fn kick(viewer_id: &str) -> BanLedgerKey {
    key(ViewerPlatform::Kick, CHANNEL, viewer_id)
}

async fn raw_row_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM ban_ledger")
        .fetch_one(pool)
        .await
        .expect("a row count")
}

#[tokio::test]
async fn upsert_into_an_empty_ledger_stores_the_entry_for_get() {
    let ledger = ledger().await;
    let entry = ban(kick("v1"), at(0), Some(TEN_MINUTES));

    let returned = ledger.repo.upsert(&entry, at(0)).await.unwrap();

    assert_eq!(returned, entry);
    assert_eq!(
        ledger.repo.get(&entry.key, at(1)).await.unwrap(),
        Some(entry)
    );
}

#[tokio::test]
async fn upsert_merges_an_observed_continuation_with_the_stored_row() {
    let ledger = ledger().await;
    let issued = ban(kick("v1"), at(0), Some(TEN_MINUTES));
    ledger.repo.upsert(&issued, at(0)).await.unwrap();
    let observed = BanLedgerEntry {
        viewer_name: "renamed".to_owned(),
        platform_ban_id: Some("platform-id".to_owned()),
        origin: BanOrigin::Observed,
        ..ban(kick("v1"), at(30), Some(TEN_MINUTES))
    };
    let expected = BanLedgerEntry {
        viewer_name: "renamed".to_owned(),
        platform_ban_id: Some("platform-id".to_owned()),
        ..issued
    };

    let returned = ledger.repo.upsert(&observed, at(30)).await.unwrap();

    assert_eq!(returned, expected);
    assert_eq!(
        ledger.repo.get(&expected.key, at(31)).await.unwrap(),
        Some(expected)
    );
}

#[tokio::test]
async fn upsert_lazily_deletes_expired_rows_of_every_key_and_keeps_active_ones() {
    let ledger = ledger().await;
    let other_channel = key(ViewerPlatform::Twitch, "elsewhere", "v2");
    for entry in [
        ban(kick("expired"), at(0), Some(TEN_MINUTES)),
        ban(other_channel, at(0), Some(TEN_MINUTES)),
        ban(kick("permanent"), at(0), None),
        ban(kick("still_active"), at(0), Some(Duration::hours(1))),
    ] {
        ledger.repo.upsert(&entry, at(0)).await.unwrap();
    }

    ledger
        .repo
        .upsert(&ban(kick("fresh"), at(600), None), at(600))
        .await
        .unwrap();

    assert_eq!(raw_row_count(&ledger.pool).await, 3);
}

#[tokio::test]
async fn get_excludes_a_ban_from_its_expiry_instant_onward() {
    let ledger = ledger().await;
    let entry = ban(kick("v1"), at(0), Some(TEN_MINUTES));
    ledger.repo.upsert(&entry, at(0)).await.unwrap();
    let expiry = at(600);

    for (label, now, visible) in [
        (
            "one millisecond before expiry",
            expiry - Duration::MILLISECOND,
            true,
        ),
        ("at expiry", expiry, false),
        ("after expiry", expiry + Duration::HOUR, false),
    ] {
        let found = ledger.repo.get(&entry.key, now).await.unwrap();
        assert_eq!(found.is_some(), visible, "{label}");
    }
}

#[tokio::test]
async fn list_active_skips_expired_bans_and_orders_newest_first_then_by_viewer_id() {
    let ledger = ledger().await;
    for entry in [
        ban(kick("b_same_time"), at(100), None),
        ban(kick("oldest"), at(0), None),
        ban(kick("a_same_time"), at(100), Some(TEN_MINUTES)),
        ban(kick("newest"), at(200), None),
        ban(kick("expires_at_now"), at(0), Some(Duration::seconds(300))),
    ] {
        ledger.repo.upsert(&entry, at(0)).await.unwrap();
    }

    let listed = ledger
        .repo
        .list_active(ViewerPlatform::Kick, CHANNEL, at(300), usize::MAX)
        .await
        .unwrap();

    let order: Vec<&str> = listed.iter().map(|e| e.key.viewer_id.as_str()).collect();
    assert_eq!(order, ["newest", "a_same_time", "b_same_time", "oldest"]);
}

#[tokio::test]
async fn list_active_clamps_the_limit_to_the_cap() {
    let ledger = ledger().await;
    sqlx::query(
        "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq WHERE n < ?)
         INSERT INTO ban_ledger
            (platform, channel_id, viewer_id, viewer_name, banned_at, origin)
         SELECT 'kick', ?, 'viewer' || n, 'name', n, 'observed' FROM seq",
    )
    .bind(i64::try_from(BAN_LEDGER_LIST_CAP + 1).unwrap())
    .bind(CHANNEL)
    .execute(&ledger.pool)
    .await
    .expect("seeded bans");

    for (label, limit, expected) in [
        ("zero", 0, 0),
        (
            "one under the cap",
            BAN_LEDGER_LIST_CAP - 1,
            BAN_LEDGER_LIST_CAP - 1,
        ),
        ("at the cap", BAN_LEDGER_LIST_CAP, BAN_LEDGER_LIST_CAP),
        (
            "one over the cap",
            BAN_LEDGER_LIST_CAP + 1,
            BAN_LEDGER_LIST_CAP,
        ),
        ("unbounded", usize::MAX, BAN_LEDGER_LIST_CAP),
    ] {
        let listed = ledger
            .repo
            .list_active(ViewerPlatform::Kick, CHANNEL, at(0), limit)
            .await
            .unwrap();
        assert_eq!(listed.len(), expected, "{label}");
    }
}

#[tokio::test]
async fn ledger_keys_are_isolated_by_platform_and_channel() {
    let ledger = ledger().await;
    let kick_here = kick("shared_viewer");
    let kick_other_channel = key(ViewerPlatform::Kick, "other_channel", "shared_viewer");
    let youtube_here = key(ViewerPlatform::YouTube, CHANNEL, "shared_viewer");
    for entry_key in [&kick_here, &kick_other_channel, &youtube_here] {
        ledger
            .repo
            .upsert(&ban(entry_key.clone(), at(0), None), at(0))
            .await
            .unwrap();
    }

    assert!(ledger.repo.remove(&kick_here).await.unwrap());

    let remaining_kick = ledger
        .repo
        .list_active(ViewerPlatform::Kick, "other_channel", at(0), usize::MAX)
        .await
        .unwrap();
    let remaining_youtube = ledger
        .repo
        .list_active(ViewerPlatform::YouTube, CHANNEL, at(0), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        (
            ledger.repo.get(&kick_here, at(0)).await.unwrap(),
            remaining_kick.len(),
            remaining_youtube.len(),
        ),
        (None, 1, 1)
    );
}

#[tokio::test]
async fn remove_reports_whether_a_row_existed() {
    let ledger = ledger().await;
    let entry = ban(kick("v1"), at(0), None);
    ledger.repo.upsert(&entry, at(0)).await.unwrap();

    let first = ledger.repo.remove(&entry.key).await.unwrap();
    let second = ledger.repo.remove(&entry.key).await.unwrap();
    let never_stored = ledger.repo.remove(&kick("unknown")).await.unwrap();

    assert_eq!((first, second, never_stored), (true, false, false));
}

#[tokio::test]
async fn upsert_truncates_timestamps_to_milliseconds_consistently_with_get() {
    let ledger = ledger().await;
    let sub_milli = Duration::nanoseconds(123_456_789);
    let entry = ban(kick("v1"), at(0) + sub_milli, Some(TEN_MINUTES));
    let truncated = BanLedgerEntry {
        banned_at: at(0) + Duration::milliseconds(123),
        expires_at: Some(at(600) + Duration::milliseconds(123)),
        ..entry.clone()
    };

    let returned = ledger.repo.upsert(&entry, at(0)).await.unwrap();
    let fetched = ledger.repo.get(&entry.key, at(1)).await.unwrap();

    assert_eq!((returned, fetched), (truncated.clone(), Some(truncated)));
}

#[tokio::test]
async fn migration_adds_the_ban_ledger_and_its_indexes_without_touching_existing_rows() {
    let (pool, _dir) = empty_pool().await;
    MIGRATIONS
        .run_to(VERSION_BEFORE_BAN_LEDGER, &pool)
        .await
        .expect("the schema before the ban ledger");
    sqlx::query(
        "INSERT INTO viewers (platform, viewer_id, username, first_seen_at, last_seen_at)
         VALUES ('kick', '42', 'existing', 7, 9)",
    )
    .execute(&pool)
    .await
    .expect("a viewer row");

    apply_migrations(&pool).await.expect("migrate");

    let mut objects: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE tbl_name = 'ban_ledger' AND sql IS NOT NULL",
    )
    .fetch_all(&pool)
    .await
    .expect("schema objects");
    objects.sort();
    let viewer: (String, i64, i64) = sqlx::query_as(
        "SELECT username, first_seen_at, last_seen_at FROM viewers WHERE viewer_id = '42'",
    )
    .fetch_one(&pool)
    .await
    .expect("the existing viewer");
    assert_eq!(
        objects,
        [
            "ban_ledger",
            "idx_ban_ledger_channel_banned_at",
            "idx_ban_ledger_expires_at"
        ]
    );
    assert_eq!(viewer, ("existing".to_owned(), 7, 9));
}
