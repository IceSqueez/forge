#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use common::{Sandboxed, TEST_KEY, sandboxed_backend};
use forge_storage::{DataProvider, DonationRepo, StorageError, StoredDonation};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{CurrencyCode, Donation, DonationOrigin, Donor, IntegrationId, MoneyAmount};
use time::{Duration, OffsetDateTime};

const PROVIDER: &str = "donatello";
const OTHER_PROVIDER: &str = "monobank";
const DONOR_NAME: &str = "SecretDonorName";
const MESSAGE: &str = "private message body";
const CONCURRENT_INSERTS: usize = 16;

async fn backend() -> Sandboxed<SqliteBackend> {
    sandboxed_backend(":memory:", TEST_KEY).await
}

fn repo(backend: &SqliteBackend) -> Arc<dyn DonationRepo> {
    backend.donation_repo()
}

fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_790_000_000 + seconds).expect("a valid instant")
}

fn uah() -> CurrencyCode {
    CurrencyCode::parse("UAH").expect("a currency")
}

fn donation(id: &str, occurred_at: OffsetDateTime, origin: DonationOrigin) -> Donation {
    Donation {
        provider: IntegrationId::new(PROVIDER),
        donation_id: id.to_owned(),
        donor: Donor::Named(DONOR_NAME.to_owned()),
        message: Some(MESSAGE.to_owned()),
        amount: MoneyAmount::from_micros(150_500_000, uah()),
        occurred_at,
        origin,
    }
}

fn live(id: &str, occurred_at: OffsetDateTime) -> Donation {
    donation(id, occurred_at, DonationOrigin::Live)
}

fn ids(stored: &[StoredDonation]) -> Vec<&str> {
    stored
        .iter()
        .map(|s| s.donation.donation_id.as_str())
        .collect()
}

async fn stored(repo: &Arc<dyn DonationRepo>, id: &str) -> StoredDonation {
    repo.list_recent(usize::MAX)
        .await
        .expect("list")
        .into_iter()
        .find(|s| s.donation.donation_id == id)
        .expect("the stored donation")
}

#[tokio::test]
async fn insert_if_new_records_a_first_seen_donation_and_reads_it_back_unchanged() {
    let backend = backend().await;
    let repo = repo(&backend);
    let original = live("d-1", at(0));

    let inserted = repo.insert_if_new(&original, at(5)).await.expect("insert");

    assert!(inserted);
    assert_eq!(
        repo.list_recent(10).await.expect("list"),
        vec![StoredDonation {
            donation: original,
            received_at: at(5),
            announced_at: None,
        }]
    );
}

#[tokio::test]
async fn insert_if_new_reports_a_duplicate_and_keeps_the_first_copy() {
    let backend = backend().await;
    let repo = repo(&backend);
    repo.insert_if_new(&live("d-1", at(0)), at(5))
        .await
        .expect("first");
    let mut redelivered = live("d-1", at(0));
    redelivered.message = Some("changed".to_owned());

    let inserted = repo
        .insert_if_new(&redelivered, at(9))
        .await
        .expect("second");

    let kept = stored(&repo, "d-1").await;
    assert!(!inserted);
    assert_eq!(
        (kept.donation.message.as_deref(), kept.received_at),
        (Some(MESSAGE), at(5))
    );
}

#[tokio::test]
async fn insert_if_new_treats_the_same_id_from_another_provider_as_new() {
    let backend = backend().await;
    let repo = repo(&backend);
    repo.insert_if_new(&live("d-1", at(0)), at(5))
        .await
        .expect("first");
    let mut other = live("d-1", at(0));
    other.provider = IntegrationId::new(OTHER_PROVIDER);

    let inserted = repo.insert_if_new(&other, at(5)).await.expect("other");

    assert!(inserted);
    assert_eq!(repo.list_recent(10).await.expect("list").len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_inserts_of_one_donation_report_new_exactly_once() {
    let backend = backend().await;
    let repo = repo(&backend);
    let tasks: Vec<_> = (0..CONCURRENT_INSERTS)
        .map(|_| {
            let repo = Arc::clone(&repo);
            tokio::spawn(async move { repo.insert_if_new(&live("d-1", at(0)), at(5)).await })
        })
        .collect();

    let mut reported_new = 0;
    for task in tasks {
        if task.await.expect("join").expect("insert") {
            reported_new += 1;
        }
    }

    assert_eq!(
        (
            reported_new,
            repo.list_recent(100).await.expect("list").len()
        ),
        (1, 1)
    );
}

#[tokio::test]
async fn insert_if_new_rejects_a_test_donation_without_recording_it() {
    let backend = backend().await;
    let repo = repo(&backend);

    let result = repo
        .insert_if_new(&donation("d-1", at(0), DonationOrigin::Test), at(5))
        .await;

    assert!(
        matches!(result, Err(StorageError::ValidationFailed { ref field, .. }) if field == "origin"),
        "{result:?}"
    );
    assert!(repo.list_recent(10).await.expect("list").is_empty());
}

#[tokio::test]
async fn insert_if_new_accepts_the_largest_storable_amount() {
    let backend = backend().await;
    let repo = repo(&backend);
    let mut largest = live("d-1", at(0));
    largest.amount = MoneyAmount::from_micros(i64::MAX as u64, uah());

    repo.insert_if_new(&largest, at(5)).await.expect("insert");

    assert_eq!(stored(&repo, "d-1").await.donation.amount, largest.amount);
}

#[tokio::test]
async fn insert_if_new_rejects_an_amount_one_micro_past_the_ledger_range() {
    let backend = backend().await;
    let repo = repo(&backend);
    let mut too_large = live("d-1", at(0));
    too_large.amount = MoneyAmount::from_micros(i64::MAX as u64 + 1, uah());

    let result = repo.insert_if_new(&too_large, at(5)).await;

    assert!(
        matches!(result, Err(StorageError::ValidationFailed { ref field, .. }) if field == "amount"),
        "{result:?}"
    );
}

#[tokio::test]
async fn a_history_donation_is_stored_as_already_announced_at_its_receipt() {
    let backend = backend().await;
    let repo = repo(&backend);

    repo.insert_if_new(&donation("d-1", at(0), DonationOrigin::History), at(5))
        .await
        .expect("insert");

    assert_eq!(stored(&repo, "d-1").await.announced_at, Some(at(5)));
}

#[tokio::test]
async fn every_donor_visibility_round_trips_through_the_ledger() {
    let backend = backend().await;
    let repo = repo(&backend);
    let donors = [
        ("named", Donor::Named(DONOR_NAME.to_owned())),
        ("anonymous", Donor::Anonymous),
        ("hidden", Donor::Hidden),
    ];
    for (id, donor) in &donors {
        let mut d = live(id, at(0));
        d.donor = donor.clone();
        repo.insert_if_new(&d, at(5)).await.expect("insert");
    }

    for (id, donor) in donors {
        assert_eq!(stored(&repo, id).await.donation.donor, donor, "{id}");
    }
}

#[tokio::test]
async fn a_donation_without_a_message_reads_back_without_one() {
    let backend = backend().await;
    let repo = repo(&backend);
    let mut silent = live("d-1", at(0));
    silent.message = None;

    repo.insert_if_new(&silent, at(5)).await.expect("insert");

    assert_eq!(stored(&repo, "d-1").await.donation.message, None);
}

#[tokio::test]
async fn mark_announced_stamps_an_unannounced_donation_once() {
    let backend = backend().await;
    let repo = repo(&backend);
    let provider = IntegrationId::new(PROVIDER);
    repo.insert_if_new(&live("d-1", at(0)), at(5))
        .await
        .expect("insert");

    let first = repo
        .mark_announced(&provider, "d-1", at(7))
        .await
        .expect("mark");
    let repeat = repo
        .mark_announced(&provider, "d-1", at(9))
        .await
        .expect("mark again");

    assert_eq!(
        (first, repeat, stored(&repo, "d-1").await.announced_at),
        (true, false, Some(at(7)))
    );
}

#[tokio::test]
async fn mark_announced_reports_false_for_an_unknown_donation() {
    let backend = backend().await;
    let repo = repo(&backend);
    repo.insert_if_new(&live("d-1", at(0)), at(5))
        .await
        .expect("insert");

    let unknown_id = repo
        .mark_announced(&IntegrationId::new(PROVIDER), "d-2", at(7))
        .await
        .expect("mark");
    let other_provider = repo
        .mark_announced(&IntegrationId::new(OTHER_PROVIDER), "d-1", at(7))
        .await
        .expect("mark");

    assert_eq!(
        (
            unknown_id,
            other_provider,
            stored(&repo, "d-1").await.announced_at
        ),
        (false, false, None)
    );
}

#[tokio::test]
async fn list_recent_returns_newest_occurrence_first_and_the_later_insert_first_on_ties() {
    let backend = backend().await;
    let repo = repo(&backend);
    for (id, occurred) in [
        ("old", 0),
        ("tie-first", 10),
        ("newest", 20),
        ("tie-second", 10),
    ] {
        repo.insert_if_new(&live(id, at(occurred)), at(30))
            .await
            .expect("insert");
    }

    let listed = repo.list_recent(10).await.expect("list");

    assert_eq!(ids(&listed), ["newest", "tie-second", "tie-first", "old"]);
}

#[tokio::test]
async fn list_recent_caps_the_result_at_the_limit_keeping_the_newest() {
    let backend = backend().await;
    let repo = repo(&backend);
    for (id, occurred) in [("a", 0), ("b", 10), ("c", 20)] {
        repo.insert_if_new(&live(id, at(occurred)), at(30))
            .await
            .expect("insert");
    }

    let limited: Vec<Vec<String>> = {
        let mut out = Vec::new();
        for limit in [0, 2, 3, 4] {
            let listed = repo.list_recent(limit).await.expect("list");
            out.push(ids(&listed).into_iter().map(str::to_owned).collect());
        }
        out
    };

    assert_eq!(
        limited,
        vec![
            vec![],
            vec!["c".to_owned(), "b".to_owned()],
            vec!["c".to_owned(), "b".to_owned(), "a".to_owned()],
            vec!["c".to_owned(), "b".to_owned(), "a".to_owned()],
        ]
    );
}

#[tokio::test]
async fn has_donations_from_answers_per_provider() {
    let backend = backend().await;
    let repo = repo(&backend);
    let empty_before = repo
        .has_donations_from(&IntegrationId::new(PROVIDER))
        .await
        .expect("query");
    repo.insert_if_new(&live("d-1", at(0)), at(5))
        .await
        .expect("insert");

    let recorded = repo
        .has_donations_from(&IntegrationId::new(PROVIDER))
        .await
        .expect("query");
    let other = repo
        .has_donations_from(&IntegrationId::new(OTHER_PROVIDER))
        .await
        .expect("query");

    assert_eq!((empty_before, recorded, other), (false, true, false));
}

#[tokio::test]
async fn catch_up_lists_unannounced_donations_from_the_cutoff_oldest_first() {
    let backend = backend().await;
    let repo = repo(&backend);
    let cutoff = at(100);
    repo.insert_if_new(
        &live("just-before", cutoff - Duration::milliseconds(1)),
        at(200),
    )
    .await
    .expect("insert");
    repo.insert_if_new(&live("later", at(150)), at(200))
        .await
        .expect("insert");
    repo.insert_if_new(&live("at-cutoff", cutoff), at(200))
        .await
        .expect("insert");
    repo.insert_if_new(&live("announced", at(120)), at(200))
        .await
        .expect("insert");
    repo.mark_announced(&IntegrationId::new(PROVIDER), "announced", at(201))
        .await
        .expect("mark");

    let pending = repo
        .list_unannounced_occurred_since(cutoff)
        .await
        .expect("catch-up");

    assert_eq!(ids(&pending), ["at-cutoff", "later"]);
}

#[tokio::test]
async fn catch_up_excludes_history_donations() {
    let backend = backend().await;
    let repo = repo(&backend);
    repo.insert_if_new(&donation("past", at(150), DonationOrigin::History), at(200))
        .await
        .expect("insert");

    let pending = repo
        .list_unannounced_occurred_since(at(100))
        .await
        .expect("catch-up");

    assert!(pending.is_empty(), "{:?}", ids(&pending));
}

#[tokio::test]
async fn mark_announced_all_occurred_before_settles_only_stale_unannounced_donations() {
    let backend = backend().await;
    let repo = repo(&backend);
    let cutoff = at(100);
    for (id, occurred) in [("stale-a", at(10)), ("stale-b", at(50)), ("fresh", cutoff)] {
        repo.insert_if_new(&live(id, occurred), at(200))
            .await
            .expect("insert");
    }
    repo.insert_if_new(&live("already", at(20)), at(200))
        .await
        .expect("insert");
    repo.mark_announced(&IntegrationId::new(PROVIDER), "already", at(201))
        .await
        .expect("mark");

    let settled = repo
        .mark_announced_all_occurred_before(cutoff, at(300))
        .await
        .expect("settle");

    let mut announced = Vec::new();
    for id in ["stale-a", "stale-b", "fresh", "already"] {
        announced.push(stored(&repo, id).await.announced_at);
    }
    assert_eq!(settled, 2);
    assert_eq!(
        announced,
        vec![Some(at(300)), Some(at(300)), None, Some(at(201))]
    );
}

#[test]
fn stored_donation_debug_hides_the_donor_name_and_message() {
    let stored = StoredDonation {
        donation: live("d-1", at(0)),
        received_at: at(5),
        announced_at: None,
    };

    let rendered = format!("{stored:?}");

    assert!(
        !rendered.contains(DONOR_NAME) && !rendered.contains(MESSAGE),
        "{rendered}"
    );
}
