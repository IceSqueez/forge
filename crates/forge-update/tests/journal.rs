#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use forge_update::{JOURNAL_FORMAT, UpdateError, UpdateJournal};

fn journal() -> UpdateJournal {
    UpdateJournal {
        format: JOURNAL_FORMAT,
        from_version: "0.6.1".to_owned(),
        to_version: "0.6.2".to_owned(),
        install_target: PathBuf::from("/opt/Forge ü/forge.AppImage"),
        previous_build: PathBuf::from("/opt/Forge ü/.forge-previous"),
        database_snapshot: PathBuf::from("/data/forge/pre-update.sqlite"),
        schema_version: 42,
    }
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn a_written_journal_reads_back_unchanged() {
    let data = tempfile::tempdir().unwrap();

    journal().write(data.path()).await.unwrap();

    assert_eq!(
        UpdateJournal::read(data.path()).await.unwrap(),
        Some(journal())
    );
}

#[tokio::test]
async fn writing_leaves_only_the_journal_file_behind() {
    let data = tempfile::tempdir().unwrap();

    journal().write(data.path()).await.unwrap();

    assert_eq!(entries(data.path()), ["update-journal.json"]);
}

#[tokio::test]
async fn a_second_write_replaces_the_first_journal() {
    let data = tempfile::tempdir().unwrap();
    journal().write(data.path()).await.unwrap();
    let mut newer = journal();
    newer.to_version = "0.6.3".to_owned();

    newer.write(data.path()).await.unwrap();

    assert_eq!(UpdateJournal::read(data.path()).await.unwrap(), Some(newer));
}

#[tokio::test]
async fn writing_creates_a_missing_data_dir() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("not").join("yet");

    journal().write(&data).await.unwrap();

    assert_eq!(UpdateJournal::read(&data).await.unwrap(), Some(journal()));
}

#[tokio::test]
async fn reading_without_a_journal_file_yields_none() {
    let data = tempfile::tempdir().unwrap();

    assert_eq!(UpdateJournal::read(data.path()).await.unwrap(), None);
}

#[tokio::test]
async fn clearing_removes_the_journal_and_a_second_clear_is_harmless() {
    let data = tempfile::tempdir().unwrap();
    journal().write(data.path()).await.unwrap();

    UpdateJournal::clear(data.path()).await.unwrap();
    UpdateJournal::clear(data.path()).await.unwrap();

    assert_eq!(UpdateJournal::read(data.path()).await.unwrap(), None);
}

#[tokio::test]
async fn a_corrupt_or_foreign_journal_is_reported_as_malformed() {
    let mut future_format = serde_json::to_value(journal()).unwrap();
    future_format["format"] = serde_json::json!(JOURNAL_FORMAT + 1);
    let mut missing_field = serde_json::to_value(journal()).unwrap();
    missing_field
        .as_object_mut()
        .unwrap()
        .remove("schema_version");
    let truncated = serde_json::to_vec(&journal()).unwrap()[..40].to_vec();
    for (label, bytes) in [
        ("empty", Vec::new()),
        ("binary", vec![0xff, 0x00, 0xfe]),
        ("truncated", truncated),
        ("future format", serde_json::to_vec(&future_format).unwrap()),
        ("missing field", serde_json::to_vec(&missing_field).unwrap()),
    ] {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(UpdateJournal::path_in(data.path()), bytes).unwrap();

        let result = UpdateJournal::read(data.path()).await;

        assert!(
            matches!(result, Err(UpdateError::MalformedJournal(_))),
            "{label}: {result:?}",
        );
    }
}
