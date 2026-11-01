#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
use std::sync::Arc;

use forge_storage::test_support::MemoryCredentials;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_storage_sqlite::SqliteBackend;
use tempfile::TempDir;

const TEST_KEY: [u8; 32] = [0x42; 32];

struct Subject {
    name: &'static str,
    repo: Arc<dyn CredentialsRepo>,
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
            repo: Arc::new(MemoryCredentials::default()),
            _media_root: None,
        },
        Subject {
            name: "sqlite",
            repo: forge_storage::DataProvider::credentials_repo(&sqlite),
            _media_root: Some(media_root),
        },
    ]
}

fn id(name: &str) -> CredentialId {
    CredentialId::new(name)
}

#[tokio::test]
async fn a_bundle_is_read_back_verbatim_and_the_last_write_wins() {
    for subject in every_implementation().await {
        let repo = &subject.repo;

        repo.store(&id("kick:broadcaster"), "first").await.unwrap();
        repo.store(&id("kick:broadcaster"), "second ю")
            .await
            .unwrap();

        assert_eq!(
            repo.load(&id("kick:broadcaster")).await.unwrap().as_deref(),
            Some("second ю"),
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn an_absent_id_loads_nothing_and_has_no_refresh_time() {
    for subject in every_implementation().await {
        let repo = &subject.repo;

        assert_eq!(
            repo.load(&id("absent")).await.unwrap(),
            None,
            "{}",
            subject.name
        );
        assert_eq!(
            repo.last_refresh(&id("absent")).await.unwrap(),
            None,
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn delete_reports_whether_a_row_existed_and_removes_it() {
    for subject in every_implementation().await {
        let repo = &subject.repo;
        repo.store(&id("obs:default"), "bundle").await.unwrap();

        let first = repo.delete(&id("obs:default")).await.unwrap();
        let second = repo.delete(&id("obs:default")).await.unwrap();

        assert_eq!(
            (first, second, repo.load(&id("obs:default")).await.unwrap()),
            (true, false, None),
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn ids_are_listed_in_order_whatever_the_store_order() {
    for subject in every_implementation().await {
        let repo = &subject.repo;
        for name in ["youtube:broadcaster", "kick:broadcaster", "obs:default"] {
            repo.store(&id(name), "bundle").await.unwrap();
        }

        let listed: Vec<String> = repo
            .list_ids()
            .await
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect();

        assert_eq!(
            listed,
            ["kick:broadcaster", "obs:default", "youtube:broadcaster"],
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn storing_a_bundle_stamps_its_refresh_time() {
    for subject in every_implementation().await {
        let repo = &subject.repo;

        repo.store(&id("twitch:bot"), "bundle").await.unwrap();

        assert!(
            repo.last_refresh(&id("twitch:bot"))
                .await
                .unwrap()
                .is_some(),
            "{}",
            subject.name
        );
    }
}

#[tokio::test]
async fn marking_an_absent_id_refreshed_is_refused_by_name() {
    for subject in every_implementation().await {
        let refused = subject.repo.mark_refreshed(&id("absent")).await;

        assert!(
            matches!(&refused, Err(StorageError::NotFound { key }) if key == "absent"),
            "{}: {refused:?}",
            subject.name
        );
    }
}
