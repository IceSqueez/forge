#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
use std::sync::Arc;

use forge_storage::test_support::MemoryGlobals;
use forge_storage::{GlobalsRepo, StorageError};
use forge_storage_sqlite::SqliteBackend;
use forge_types::Variant;
use tempfile::TempDir;

const TEST_KEY: [u8; 32] = [0x42; 32];
const COUNTER: &str = "stream.deaths";
const OTHER: &str = "stream.wins";

struct Subject {
    name: &'static str,
    repo: Arc<dyn GlobalsRepo>,
    _media_root: Option<TempDir>,
}

async fn every_implementation() -> [Subject; 2] {
    let media_root = tempfile::tempdir().unwrap();
    let sqlite = SqliteBackend::open_for_test(":memory:", TEST_KEY, media_root.path().into(), None)
        .await
        .unwrap();
    [
        Subject {
            name: "memory",
            repo: Arc::new(MemoryGlobals::default()),
            _media_root: None,
        },
        Subject {
            name: "sqlite",
            repo: forge_storage::DataProvider::globals_repo(&sqlite),
            _media_root: Some(media_root),
        },
    ]
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

#[tokio::test]
async fn an_absent_global_reads_as_nothing_and_has_no_persistence_flag() {
    for subject in every_implementation().await {
        assert_eq!(
            subject.repo.get(COUNTER).await.unwrap(),
            None,
            "{}",
            subject.name
        );
        assert_eq!(
            subject.repo.persisted(COUNTER).await.unwrap(),
            None,
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn a_set_global_reads_back_with_the_flag_it_was_written_with() {
    for subject in every_implementation().await {
        for persisted in [true, false] {
            subject
                .repo
                .set(COUNTER, Variant::Int(3), persisted)
                .await
                .unwrap();

            assert_eq!(
                (
                    subject.repo.get(COUNTER).await.unwrap(),
                    subject.repo.persisted(COUNTER).await.unwrap(),
                ),
                (Some(Variant::Int(3)), Some(persisted)),
                "{}",
                subject.name
            );
        }
    }
}

#[tokio::test]
async fn delete_reports_whether_a_global_existed() {
    for subject in every_implementation().await {
        subject.repo.set(COUNTER, text("x"), true).await.unwrap();

        assert!(
            subject.repo.delete(COUNTER).await.unwrap(),
            "{}",
            subject.name
        );
        assert!(
            !subject.repo.delete(COUNTER).await.unwrap(),
            "{}",
            subject.name
        );
        assert_eq!(
            subject.repo.get(COUNTER).await.unwrap(),
            None,
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn list_names_every_global_with_its_value_and_flag() {
    for subject in every_implementation().await {
        subject.repo.set(OTHER, text("a"), false).await.unwrap();
        subject
            .repo
            .set(COUNTER, Variant::Int(1), true)
            .await
            .unwrap();

        let mut listed: Vec<_> = subject
            .repo
            .list()
            .await
            .unwrap()
            .into_iter()
            .map(|entry| (entry.name, entry.value, entry.persisted))
            .collect();
        listed.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(
            listed,
            [
                (COUNTER.to_owned(), Variant::Int(1), true),
                (OTHER.to_owned(), text("a"), false),
            ],
            "{}",
            subject.name
        );
        assert_eq!(subject.repo.count().await.unwrap(), 2, "{}", subject.name);
    }
}

#[tokio::test]
async fn incrementing_adds_to_the_stored_number_and_keeps_its_kind_and_flag() {
    for subject in every_implementation().await {
        subject
            .repo
            .set(COUNTER, Variant::Int(40), true)
            .await
            .unwrap();
        subject
            .repo
            .set(OTHER, Variant::float(1.5).unwrap(), false)
            .await
            .unwrap();

        assert_eq!(
            subject.repo.incr(COUNTER, 2).await.unwrap(),
            Variant::Int(42),
            "{}",
            subject.name
        );
        assert_eq!(
            subject.repo.incr(OTHER, -1).await.unwrap(),
            Variant::float(0.5).unwrap(),
            "{}",
            subject.name
        );
        assert_eq!(
            (
                subject.repo.get(COUNTER).await.unwrap(),
                subject.repo.persisted(COUNTER).await.unwrap(),
            ),
            (Some(Variant::Int(42)), Some(true)),
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn incrementing_an_absent_global_is_refused() {
    for subject in every_implementation().await {
        let refused = subject.repo.incr(COUNTER, 1).await;

        assert!(
            matches!(&refused, Err(StorageError::NotFound { key }) if key == COUNTER),
            "{}: {refused:?}",
            subject.name
        );
    }
}

#[tokio::test]
async fn incrementing_a_non_number_is_refused_and_leaves_it_alone() {
    for subject in every_implementation().await {
        subject.repo.set(COUNTER, text("x"), true).await.unwrap();

        let refused = subject.repo.incr(COUNTER, 1).await;

        assert!(
            matches!(&refused, Err(StorageError::TypeMismatch { name, .. }) if name == COUNTER),
            "{}: {refused:?}",
            subject.name
        );
        assert_eq!(
            subject.repo.get(COUNTER).await.unwrap(),
            Some(text("x")),
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn renaming_moves_the_value_and_refuses_a_taken_or_absent_name() {
    for subject in every_implementation().await {
        subject
            .repo
            .set(COUNTER, Variant::Int(1), true)
            .await
            .unwrap();
        subject
            .repo
            .set(OTHER, Variant::Int(2), false)
            .await
            .unwrap();

        let taken = subject.repo.rename(COUNTER, OTHER).await;
        assert!(
            matches!(&taken, Err(StorageError::NameCollision { name }) if name == OTHER),
            "{}: {taken:?}",
            subject.name
        );
        let absent = subject.repo.rename("no.such.global", "still.none").await;
        assert!(
            matches!(&absent, Err(StorageError::NotFound { .. })),
            "{}: {absent:?}",
            subject.name
        );

        subject.repo.rename(COUNTER, "stream.lives").await.unwrap();
        assert_eq!(
            (
                subject.repo.get(COUNTER).await.unwrap(),
                subject.repo.get("stream.lives").await.unwrap(),
                subject.repo.persisted("stream.lives").await.unwrap(),
            ),
            (None, Some(Variant::Int(1)), Some(true)),
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn appending_trims_the_oldest_items_to_the_cap_and_reports_the_length() {
    for subject in every_implementation().await {
        for item in 1..=3 {
            let length = subject
                .repo
                .array_append(COUNTER, Variant::Int(item), Some(2))
                .await
                .unwrap();
            assert_eq!(
                length,
                usize::try_from(item.min(2)).unwrap(),
                "{}",
                subject.name
            );
        }

        assert_eq!(
            subject.repo.get(COUNTER).await.unwrap(),
            Some(Variant::Array(vec![Variant::Int(2), Variant::Int(3)])),
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn appending_to_a_non_array_is_refused() {
    for subject in every_implementation().await {
        subject.repo.set(COUNTER, text("x"), true).await.unwrap();

        let refused = subject
            .repo
            .array_append(COUNTER, Variant::Int(1), None)
            .await;

        assert!(
            matches!(&refused, Err(StorageError::TypeMismatch { name, .. }) if name == COUNTER),
            "{}: {refused:?}",
            subject.name
        );
    }
}
