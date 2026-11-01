#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
use std::sync::Arc;

use forge_storage::test_support::{MemorySettings, OPEN_ROW_GATE};
use forge_storage::{RowClasses, SettingDisclosure, SettingPortability, SettingsRepo};
use forge_storage_sqlite::SqliteBackend;
use tempfile::TempDir;

const TEST_KEY: [u8; 32] = [0x42; 32];
const KEY: &str = "conformance.value";
const PORTABLE: RowClasses = RowClasses {
    portability: SettingPortability::Portable,
    disclosure: SettingDisclosure::Verbatim,
};
const PRIVATE: RowClasses = RowClasses::FAIL_CLOSED;

struct Subject {
    name: &'static str,
    repo: Arc<dyn SettingsRepo>,
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
            repo: Arc::new(MemorySettings::new()),
            _media_root: None,
        },
        Subject {
            name: "sqlite",
            repo: Arc::new(sqlite),
            _media_root: Some(media_root),
        },
    ]
}

async fn stored_row(repo: &dyn SettingsRepo, key: &str) -> Option<(String, RowClasses)> {
    repo.load_rows(OPEN_ROW_GATE)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.key() == key)
        .map(|row| (row.value().to_owned(), row.classes()))
}

#[tokio::test]
async fn an_absent_key_reads_as_nothing() {
    for subject in every_implementation().await {
        assert_eq!(
            subject.repo.get_raw(OPEN_ROW_GATE, KEY).await.unwrap(),
            None,
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn a_rewritten_row_takes_the_new_value_and_the_new_classes() {
    for subject in every_implementation().await {
        let repo = subject.repo.as_ref();
        repo.put_row(OPEN_ROW_GATE, KEY, "first", PORTABLE)
            .await
            .unwrap();
        repo.put_row(OPEN_ROW_GATE, KEY, "second", PRIVATE)
            .await
            .unwrap();

        assert_eq!(
            stored_row(repo, KEY).await,
            Some(("second".to_owned(), PRIVATE)),
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn delete_reports_whether_a_row_existed_and_removes_it() {
    for subject in every_implementation().await {
        let repo = subject.repo.as_ref();
        repo.put_row(OPEN_ROW_GATE, KEY, "value", PORTABLE)
            .await
            .unwrap();

        let first = repo.delete_raw(OPEN_ROW_GATE, KEY).await.unwrap();
        let second = repo.delete_raw(OPEN_ROW_GATE, KEY).await.unwrap();

        assert_eq!(
            (
                first,
                second,
                repo.get_raw(OPEN_ROW_GATE, KEY).await.unwrap()
            ),
            (true, false, None),
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn rows_load_in_key_order() {
    for subject in every_implementation().await {
        let repo = subject.repo.as_ref();
        for key in ["zzz.conformance", "aaa.conformance", "mmm.conformance"] {
            repo.put_row(OPEN_ROW_GATE, key, "value", PORTABLE)
                .await
                .unwrap();
        }

        let keys: Vec<String> = repo
            .load_rows(OPEN_ROW_GATE)
            .await
            .unwrap()
            .iter()
            .map(|row| row.key().to_owned())
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();

        assert_eq!(keys, sorted, "{}", subject.name);
    }
}

#[tokio::test]
async fn an_empty_value_is_a_stored_value_and_not_an_absent_one() {
    for subject in every_implementation().await {
        let repo = subject.repo.as_ref();
        repo.put_row(OPEN_ROW_GATE, KEY, "", PORTABLE)
            .await
            .unwrap();

        assert_eq!(
            repo.get_raw(OPEN_ROW_GATE, KEY).await.unwrap().as_deref(),
            Some(""),
            "{}",
            subject.name
        );
    }
}
