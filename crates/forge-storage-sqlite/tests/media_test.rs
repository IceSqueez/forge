#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use forge_storage::{
    DataProvider, MediaBlobId, MediaFormat, MediaKind, MediaReferrer, MediaReferrerKind, MediaRepo,
    StorageError,
};
use forge_storage_sqlite::{SqliteBackend, SqliteMediaRepo, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const TEST_KEY: [u8; 32] = [0x5c; 32];

const TINY_GIF: &[u8] = b"GIF89a";
const TINY_GIF_ID: &str = "sha256-610f5ae4d76e332636a17bd357fd6ce99029316a99d320280d4d77a746bf29e8";
const TINY_GIF_FILE: &str =
    "sha256-610f5ae4d76e332636a17bd357fd6ce99029316a99d320280d4d77a746bf29e8.gif";
const TINY_GIF_BYTES: u64 = 6;

const OTHER_GIF: &[u8] = b"GIF87a";
const OTHER_GIF_BYTES: u64 = 6;

const TINY_WAV: &[u8] = b"RIFF\x04\x00\x00\x00WAVE";
const TINY_WAV_ID: &str = "sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8";

const PNG_HEADER: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0d";
const RANDOM: &[u8] = b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09";

const IMAGE_CAP_BYTES: usize = 10 * 1024 * 1024;
const MEDIA_SUBDIR: &str = "media";
const CLIP_ID: &str = "clip-alpha";
const OTHER_CLIP_ID: &str = "clip-beta";
const SOURCE_SLOT: &str = "source";
const ICON_SLOT: &str = "icon";
const OVERLAY_ID: &str = "alert-box";

struct Fixture {
    backend: SqliteBackend,
    root: PathBuf,
    dir: TempDir,
}

impl Fixture {
    fn media(&self) -> Arc<dyn MediaRepo> {
        self.backend.media_repo()
    }

    fn root_entries(&self) -> Vec<String> {
        entries(&self.root)
    }
}

fn entries(dir: &std::path::Path) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .map(|entry| {
            entry
                .expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into()
        })
        .collect();
    names.sort();
    names
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join(MEDIA_SUBDIR);
    let backend = SqliteBackend::open_for_test("sqlite::memory:", TEST_KEY, root.clone(), None)
        .await
        .expect("open backend against an isolated media root");
    Fixture { backend, root, dir }
}

fn clip_source() -> MediaReferrer {
    MediaReferrer::new(MediaReferrerKind::SoundboardClip, CLIP_ID, SOURCE_SLOT)
}

fn overlay_icon() -> MediaReferrer {
    MediaReferrer::new(MediaReferrerKind::Overlay, OVERLAY_ID, ICON_SLOT)
}

#[tokio::test]
async fn store_names_the_blob_after_the_sha256_of_its_content() {
    let fx = fixture().await;

    for (bytes, expected) in [(TINY_GIF, TINY_GIF_ID), (TINY_WAV, TINY_WAV_ID)] {
        let blob = fx
            .media()
            .store("fixture", bytes.to_vec())
            .await
            .expect("store accepted bytes");
        assert_eq!(blob.id.as_str(), expected);
    }
}

#[tokio::test]
async fn a_traversal_label_never_reaches_the_stored_path() {
    let fx = fixture().await;

    let blob = fx
        .media()
        .store("../../../escape.gif", TINY_GIF.to_vec())
        .await
        .expect("store a hostile label");

    assert_eq!(blob.label, "escape.gif");
    assert_eq!(entries(fx.dir.path()), vec![MEDIA_SUBDIR.to_owned()]);
    assert_eq!(fx.root_entries(), vec![TINY_GIF_FILE.to_owned()]);
}

#[tokio::test]
async fn storing_identical_content_twice_keeps_one_row_and_the_first_label() {
    let fx = fixture().await;

    let first = fx
        .media()
        .store("first.gif", TINY_GIF.to_vec())
        .await
        .expect("first store");
    let second = fx
        .media()
        .store("second.gif", TINY_GIF.to_vec())
        .await
        .expect("second store");

    assert_eq!(second.id, first.id);
    assert_eq!(second.label, "first.gif");
    assert_eq!(fx.media().list().await.expect("list").len(), 1);
    assert_eq!(fx.root_entries(), vec![TINY_GIF_FILE.to_owned()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_stores_of_identical_content_converge_on_one_row_and_one_file() {
    const RACERS: usize = 8;
    let fx = fixture().await;

    let mut racing = tokio::task::JoinSet::new();
    for index in 0..RACERS {
        let media = fx.media();
        racing.spawn(async move {
            media
                .store(&format!("racer-{index}.gif"), TINY_GIF.to_vec())
                .await
        });
    }

    while let Some(joined) = racing.join_next().await {
        let blob = joined.expect("racer task").expect("racing store");
        assert_eq!(blob.id.as_str(), TINY_GIF_ID);
    }

    assert_eq!(fx.media().list().await.expect("list").len(), 1);
    assert_eq!(fx.root_entries(), vec![TINY_GIF_FILE.to_owned()]);
}

#[tokio::test]
async fn a_refused_store_leaves_neither_a_row_nor_a_file() {
    let fx = fixture().await;

    let error = fx
        .media()
        .store("logo.mp3", PNG_HEADER.to_vec())
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            StorageError::MediaTypeMismatch {
                claimed: MediaFormat::Mp3,
                detected: MediaFormat::Png,
                ..
            }
        ),
        "png bytes named .mp3 were not refused as a mismatch: {error:?}"
    );
    assert!(fx.media().list().await.expect("list").is_empty());
    assert!(fx.root_entries().is_empty());
}

#[tokio::test]
async fn import_file_labels_the_blob_with_the_source_file_name() {
    let fx = fixture().await;
    let source = fx.dir.path().join("fanfare.gif");
    std::fs::write(&source, TINY_GIF).expect("write source");

    let blob = fx.media().import_file(&source).await.expect("import");

    assert_eq!(blob.label, "fanfare.gif");
    assert_eq!(blob.id.as_str(), TINY_GIF_ID);
}

#[tokio::test]
async fn import_file_refuses_content_outside_the_accepted_list() {
    let fx = fixture().await;
    let source = fx.dir.path().join("notes.txt");
    std::fs::write(&source, RANDOM).expect("write source");

    let error = fx.media().import_file(&source).await.unwrap_err();

    let StorageError::MediaUnsupported { label } = error else {
        panic!("random bytes were not refused as unsupported: {error:?}");
    };
    assert_eq!(label, "notes.txt");
}

#[tokio::test]
async fn import_file_refuses_a_source_one_byte_over_its_kind_cap() {
    let fx = fixture().await;
    let source = fx.dir.path().join("huge.gif");
    let mut bytes = vec![0u8; IMAGE_CAP_BYTES + 1];
    bytes[..TINY_GIF.len()].copy_from_slice(TINY_GIF);
    std::fs::write(&source, &bytes).expect("write source");

    let error = fx.media().import_file(&source).await.unwrap_err();

    let StorageError::MediaTooLarge {
        size, limit, kind, ..
    } = error
    else {
        panic!("an oversized image was not refused: {error:?}");
    };
    assert_eq!(
        (size, limit, kind),
        (
            (IMAGE_CAP_BYTES + 1) as u64,
            IMAGE_CAP_BYTES as u64,
            MediaKind::Image
        )
    );
}

#[cfg(unix)]
#[tokio::test]
async fn import_file_stops_reading_a_source_that_understates_its_length_at_the_kind_cap() {
    const OVERSHOOT: usize = 64 * 1024;
    const IMPORT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

    let fx = fixture().await;
    let source = fx.dir.path().join("stream.gif");
    let made = std::process::Command::new("mkfifo")
        .arg(&source)
        .status()
        .expect("mkfifo runs");
    assert!(made.success(), "mkfifo refused to create {source:?}");

    let feed = source.clone();
    let writer = std::thread::spawn(move || {
        let mut bytes = vec![0u8; IMAGE_CAP_BYTES + OVERSHOOT];
        bytes[..TINY_GIF.len()].copy_from_slice(TINY_GIF);
        let mut pipe = std::fs::OpenOptions::new()
            .write(true)
            .open(&feed)
            .expect("open the fifo for writing");
        let _ = std::io::Write::write_all(&mut pipe, &bytes);
    });

    let error = tokio::time::timeout(IMPORT_DEADLINE, fx.media().import_file(&source))
        .await
        .expect("the import must not stall on a source that never ends")
        .unwrap_err();
    let _ = writer.join();

    let StorageError::MediaTooLarge {
        size, limit, kind, ..
    } = error
    else {
        panic!("an endless image was not refused: {error:?}");
    };
    assert_eq!(
        (size, limit, kind),
        (
            (IMAGE_CAP_BYTES + 1) as u64,
            IMAGE_CAP_BYTES as u64,
            MediaKind::Image
        )
    );
}

#[tokio::test]
async fn import_file_reports_a_source_that_does_not_exist() {
    let fx = fixture().await;
    let missing = fx.dir.path().join("never-written.gif");

    let error = fx.media().import_file(&missing).await.unwrap_err();

    assert!(
        matches!(error, StorageError::Io(_)),
        "a vanished source did not surface as an io failure: {error:?}"
    );
}

#[tokio::test]
async fn resolve_reports_not_found_once_the_managed_file_has_vanished() {
    let fx = fixture().await;
    let blob = fx
        .media()
        .store("fanfare.gif", TINY_GIF.to_vec())
        .await
        .expect("store");
    std::fs::remove_file(fx.root.join(TINY_GIF_FILE)).expect("remove the managed file");

    let error = fx.media().resolve(&blob.id).await.unwrap_err();

    let StorageError::NotFound { key } = error else {
        panic!("a vanished managed file did not surface as not-found: {error:?}");
    };
    assert_eq!(key, TINY_GIF_ID);
}

#[tokio::test]
async fn resolve_points_at_the_content_named_file_under_the_media_root() {
    let fx = fixture().await;
    let blob = fx
        .media()
        .store("fanfare.gif", TINY_GIF.to_vec())
        .await
        .expect("store");

    let path = fx.media().resolve(&blob.id).await.expect("resolve");

    assert_eq!(path, fx.root.join(TINY_GIF_FILE));
}

#[tokio::test]
async fn delete_refuses_while_a_referrer_still_holds_the_blob() {
    let fx = fixture().await;
    let blob = fx
        .media()
        .store("fanfare.gif", TINY_GIF.to_vec())
        .await
        .expect("store");
    fx.media()
        .retain(&clip_source(), &blob.id)
        .await
        .expect("retain");

    let error = fx.media().delete(&blob.id).await.unwrap_err();

    assert!(
        matches!(error, StorageError::MediaReferenced { referrer_count: 1 }),
        "a referenced blob was not protected: {error:?}"
    );
    assert_eq!(fx.root_entries(), vec![TINY_GIF_FILE.to_owned()]);
}

#[tokio::test]
async fn delete_removes_the_row_and_the_file_once_the_last_referrer_is_released() {
    let fx = fixture().await;
    let blob = fx
        .media()
        .store("fanfare.gif", TINY_GIF.to_vec())
        .await
        .expect("store");
    fx.media()
        .retain(&clip_source(), &blob.id)
        .await
        .expect("retain");

    assert!(
        fx.media().release(&clip_source()).await.expect("release"),
        "release did not report the held slot"
    );
    assert!(fx.media().delete(&blob.id).await.expect("delete"));
    assert!(fx.media().get(&blob.id).await.expect("get").is_none());
    assert!(fx.root_entries().is_empty());
}

#[tokio::test]
async fn delete_reports_nothing_removed_for_an_unknown_blob() {
    let fx = fixture().await;

    let removed = fx
        .media()
        .delete(&MediaBlobId::from_stored(TINY_GIF_ID).expect("fixture blob id is well formed"))
        .await
        .expect("delete an unknown blob");

    assert!(!removed);
}

#[tokio::test]
async fn release_reports_nothing_held_for_an_empty_slot() {
    let fx = fixture().await;

    assert!(!fx.media().release(&clip_source()).await.expect("release"));
}

#[tokio::test]
async fn retain_refuses_a_blob_that_was_never_stored() {
    let fx = fixture().await;

    let error = fx
        .media()
        .retain(
            &clip_source(),
            &MediaBlobId::from_stored(TINY_GIF_ID).expect("fixture blob id is well formed"),
        )
        .await
        .unwrap_err();

    let StorageError::NotFound { key } = error else {
        panic!("retaining an unknown blob was not refused: {error:?}");
    };
    assert_eq!(key, TINY_GIF_ID);
}

#[tokio::test]
async fn retain_replaces_whatever_the_slot_held() {
    let fx = fixture().await;
    let first = fx
        .media()
        .store("first.gif", TINY_GIF.to_vec())
        .await
        .expect("store first");
    let second = fx
        .media()
        .store("second.gif", OTHER_GIF.to_vec())
        .await
        .expect("store second");

    fx.media()
        .retain(&clip_source(), &first.id)
        .await
        .expect("retain first");
    fx.media()
        .retain(&clip_source(), &second.id)
        .await
        .expect("retain second");

    assert_eq!(
        fx.media().blob_of(&clip_source()).await.expect("blob_of"),
        Some(second.id.clone())
    );
    assert!(
        fx.media()
            .referrers(&first.id)
            .await
            .expect("referrers")
            .is_empty()
    );
}

#[tokio::test]
async fn referrers_lists_every_slot_pointing_at_the_blob() {
    let fx = fixture().await;
    let blob = fx
        .media()
        .store("shared.gif", TINY_GIF.to_vec())
        .await
        .expect("store");

    for referrer in [clip_source(), overlay_icon()] {
        fx.media()
            .retain(&referrer, &blob.id)
            .await
            .expect("retain");
    }

    assert_eq!(
        fx.media().referrers(&blob.id).await.expect("referrers"),
        vec![overlay_icon(), clip_source()]
    );
}

#[tokio::test]
async fn blob_of_reports_nothing_for_a_slot_that_was_never_retained() {
    let fx = fixture().await;

    assert_eq!(
        fx.media().blob_of(&clip_source()).await.expect("blob_of"),
        None
    );
}

#[tokio::test]
async fn release_all_drops_every_slot_of_one_row_and_leaves_the_others() {
    let fx = fixture().await;
    let blob = fx
        .media()
        .store("shared.gif", TINY_GIF.to_vec())
        .await
        .expect("store");
    let other = MediaReferrer::new(
        MediaReferrerKind::SoundboardClip,
        OTHER_CLIP_ID,
        SOURCE_SLOT,
    );

    for referrer in [
        clip_source(),
        MediaReferrer::new(MediaReferrerKind::SoundboardClip, CLIP_ID, ICON_SLOT),
        other.clone(),
    ] {
        fx.media()
            .retain(&referrer, &blob.id)
            .await
            .expect("retain");
    }

    let dropped = fx
        .media()
        .release_all(MediaReferrerKind::SoundboardClip, CLIP_ID)
        .await
        .expect("release_all");

    assert_eq!(dropped, 2);
    assert_eq!(
        fx.media().referrers(&blob.id).await.expect("referrers"),
        vec![other]
    );
}

#[tokio::test]
async fn total_bytes_counts_each_distinct_blob_once() {
    let fx = fixture().await;

    assert_eq!(fx.media().total_bytes().await.expect("empty total"), 0);

    for bytes in [TINY_GIF, OTHER_GIF, TINY_GIF] {
        fx.media()
            .store("fixture.gif", bytes.to_vec())
            .await
            .expect("store");
    }

    assert_eq!(
        fx.media().total_bytes().await.expect("total"),
        TINY_GIF_BYTES + OTHER_GIF_BYTES
    );
}

#[tokio::test]
async fn read_returns_the_exact_bytes_that_were_stored() {
    let fx = fixture().await;
    let blob = fx
        .media()
        .store("fanfare.gif", TINY_GIF.to_vec())
        .await
        .expect("store");

    assert_eq!(fx.media().read(&blob.id).await.expect("read"), TINY_GIF);
}

const STALE_INCOMING_AGE: Duration = Duration::from_hours(1);
const ONE_MINUTE: Duration = Duration::from_mins(1);
const STARTUP_SWEEP_DEADLINE: Duration = Duration::from_secs(30);
const STARTUP_SWEEP_POLL: Duration = Duration::from_millis(10);
#[cfg(windows)]
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

async fn direct_repo(root: &Path) -> (SqliteMediaRepo, SqlitePool) {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");
    (SqliteMediaRepo::new(pool.clone(), root.to_owned()), pool)
}

#[cfg(windows)]
fn open_for_timestamps(path: &Path) -> File {
    use std::os::windows::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .expect("open for timestamps")
}

#[cfg(not(windows))]
fn open_for_timestamps(path: &Path) -> File {
    File::open(path).expect("open for timestamps")
}

fn age(path: &Path, by: Duration) {
    let modified = SystemTime::now()
        .checked_sub(by)
        .expect("the clock reaches back far enough");
    open_for_timestamps(path)
        .set_modified(modified)
        .expect("set mtime");
}

fn staged_file(root: &Path, name: &str, aged_by: Duration) {
    let path = root.join(name);
    std::fs::write(&path, TINY_GIF).expect("write staged file");
    age(&path, aged_by);
}

#[tokio::test]
async fn sweep_stale_incoming_removes_only_staging_files_at_least_an_hour_old() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    let (repo, _pool) = direct_repo(root).await;
    staged_file(root, ".incoming-abandoned", STALE_INCOMING_AGE * 2);
    staged_file(root, ".incoming-on-the-threshold", STALE_INCOMING_AGE);
    staged_file(
        root,
        ".incoming-still-writing",
        STALE_INCOMING_AGE - ONE_MINUTE,
    );
    staged_file(root, "incoming-without-the-dot", STALE_INCOMING_AGE * 2);
    staged_file(root, TINY_GIF_FILE, STALE_INCOMING_AGE * 2);
    let staging_directory = root.join(".incoming-directory");
    std::fs::create_dir(&staging_directory).expect("create directory");
    age(&staging_directory, STALE_INCOMING_AGE * 2);

    let removed = repo.sweep_stale_incoming().await.expect("sweep");

    assert_eq!(removed, 2);
    assert_eq!(
        entries(root),
        vec![
            ".incoming-directory".to_owned(),
            ".incoming-still-writing".to_owned(),
            "incoming-without-the-dot".to_owned(),
            TINY_GIF_FILE.to_owned(),
        ]
    );
}

#[tokio::test]
async fn sweep_stale_incoming_of_a_root_that_does_not_exist_yet_removes_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (repo, _pool) = direct_repo(&dir.path().join(MEDIA_SUBDIR)).await;

    assert_eq!(repo.sweep_stale_incoming().await.expect("sweep"), 0);
}

#[tokio::test]
async fn opening_the_backend_sweeps_abandoned_staging_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join(MEDIA_SUBDIR);
    std::fs::create_dir(&root).expect("create media root");
    staged_file(&root, ".incoming-abandoned", STALE_INCOMING_AGE * 2);

    let _backend = SqliteBackend::open_for_test("sqlite::memory:", TEST_KEY, root.clone(), None)
        .await
        .expect("open backend");

    tokio::time::timeout(STARTUP_SWEEP_DEADLINE, async {
        while !entries(&root).is_empty() {
            tokio::time::sleep(STARTUP_SWEEP_POLL).await;
        }
    })
    .await
    .expect("the startup sweep never removed the abandoned staging file");
}

async fn insert_blob_row(pool: &SqlitePool, id: &str) {
    sqlx::query(
        "INSERT INTO media_blobs (id, format, byte_size, label, imported_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(MediaFormat::Gif.as_str())
    .bind(TINY_GIF_BYTES as i64)
    .bind("tampered.gif")
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("insert blob row");
}

const MALFORMED_STORED_IDS: [&str; 3] = [
    "../../escape",
    "SHA256-610F5AE4D76E332636A17BD357FD6CE99029316A99D320280D4D77A746BF29E8",
    "sha256-610f5ae4d76e332636a17bd357fd6ce99029316a99d320280d4d77a746bf29e",
];

#[tokio::test]
async fn a_stored_row_whose_id_is_not_a_blob_id_fails_to_decode() {
    for malformed in MALFORMED_STORED_IDS {
        let dir = tempfile::tempdir().expect("temp dir");
        let (repo, pool) = direct_repo(dir.path()).await;
        insert_blob_row(&pool, malformed).await;

        let error = repo.list().await.unwrap_err();

        assert!(
            matches!(&error, StorageError::Parse(reason) if reason.contains(malformed)),
            "{malformed:?} was answered with {error:?}"
        );
    }
}

#[tokio::test]
async fn blob_of_a_slot_holding_a_malformed_id_reports_it_as_malformed() {
    for malformed in MALFORMED_STORED_IDS {
        let dir = tempfile::tempdir().expect("temp dir");
        let (repo, pool) = direct_repo(dir.path()).await;
        insert_blob_row(&pool, malformed).await;
        let referrer = overlay_icon();
        sqlx::query(
            "INSERT INTO media_references (referrer_kind, referrer_id, slot, blob_id) VALUES (?, ?, ?, ?)",
        )
        .bind(referrer.kind.as_str())
        .bind(&referrer.id)
        .bind(&referrer.slot)
        .bind(malformed)
        .execute(&pool)
        .await
        .expect("insert reference row");

        let error = repo.blob_of(&referrer).await.unwrap_err();

        assert!(
            matches!(&error, StorageError::MalformedMediaBlobId { raw } if raw == malformed),
            "{malformed:?} was answered with {error:?}"
        );
    }
}
