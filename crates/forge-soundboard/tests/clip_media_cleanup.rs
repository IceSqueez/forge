#![expect(clippy::expect_used, reason = "test fixtures")]
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_events::EventPublisher;
use forge_events::test_support::CapturingPublisher;
use forge_soundboard::{ClipLibrary, NewClip};
use forge_storage::{DataProvider, MediaRepo, clip_source_referrer};
use forge_storage_sqlite::SqliteBackend;
use forge_storage_sqlite::test_support::TEST_KEY;
use forge_types::ClipId;
use tempfile::TempDir;

const FANFARE_WAV: &[u8] = b"RIFF\x04\x00\x00\x00WAVE";
const AIRHORN_WAV: &[u8] = b"RIFF\x08\x00\x00\x00WAVE";

struct Soundboard {
    library: ClipLibrary,
    media: Arc<dyn MediaRepo>,
    dir: TempDir,
}

async fn soundboard() -> Soundboard {
    let dir = tempfile::tempdir().expect("temp dir");
    let url = format!("sqlite://{}", dir.path().join("forge.db").display());
    let backend = SqliteBackend::open_for_test(&url, TEST_KEY, dir.path().join("media"), None)
        .await
        .expect("a file-backed backend");
    let media = backend.media_repo();
    let library = ClipLibrary::new(
        backend.soundboard_clips_repo(),
        Arc::clone(&media),
        Arc::new(CapturingPublisher::default()) as Arc<dyn EventPublisher>,
    );
    Soundboard {
        library,
        media,
        dir,
    }
}

impl Soundboard {
    fn source(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, bytes).expect("write the chosen source file");
        path
    }

    async fn add(&self, name: &str, source: &Path) -> ClipId {
        self.library
            .create_clip(NewClip {
                name: name.to_owned(),
                file_path: source.to_owned(),
                hotkey: None,
                category: "General".to_owned(),
                loop_playback: false,
                builtin_id: None,
            })
            .await
            .expect("create clip")
    }

    async fn managed_file(&self, clip: ClipId) -> PathBuf {
        let blob = self
            .media
            .blob_of(&clip_source_referrer(clip))
            .await
            .expect("blob_of")
            .expect("the clip holds its imported audio");
        self.media.resolve(&blob).await.expect("resolve")
    }
}

#[tokio::test]
async fn deleting_a_clip_removes_the_audio_file_it_alone_used() {
    let board = soundboard().await;
    let source = board.source("fanfare.wav", FANFARE_WAV);
    let clip = board.add("Fanfare", &source).await;
    let managed = board.managed_file(clip).await;

    assert!(board.library.delete_clip(clip).await.expect("delete clip"));

    assert!(
        !managed.exists(),
        "{} survived its only clip",
        managed.display()
    );
}

#[tokio::test]
async fn deleting_a_clip_keeps_audio_another_clip_still_plays() {
    let board = soundboard().await;
    let source = board.source("fanfare.wav", FANFARE_WAV);
    let doomed = board.add("Fanfare", &source).await;
    let survivor = board.add("Fanfare again", &source).await;
    let managed = board.managed_file(survivor).await;

    board
        .library
        .delete_clip(doomed)
        .await
        .expect("delete clip");

    assert!(
        managed.exists(),
        "{} went with a clip still using it",
        managed.display()
    );
}

#[tokio::test]
async fn choosing_new_audio_for_a_clip_removes_the_file_it_replaced() {
    let board = soundboard().await;
    let clip = board
        .add("Alert", &board.source("fanfare.wav", FANFARE_WAV))
        .await;
    let replaced = board.managed_file(clip).await;
    let mut row = board
        .library
        .get(clip)
        .await
        .expect("get")
        .expect("the clip row");
    row.file_path = board.source("airhorn.wav", AIRHORN_WAV);

    board.library.save_clip(&row).await.expect("save clip");

    assert!(
        !replaced.exists(),
        "{} outlived its replacement",
        replaced.display()
    );
}
