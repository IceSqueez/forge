#![expect(clippy::expect_used, reason = "test fixtures")]
mod common;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_storage::{
    DataProvider, MediaBlobId, MediaReferrer, MediaReferrerKind, MediaRepo, StorageError,
    StoreChange,
};
use forge_storage_sqlite::SqliteBackend;
use tempfile::TempDir;

use common::TEST_KEY;

const TINY_WAV: &[u8] = b"RIFF\x04\x00\x00\x00WAVE";
const OTHER_WAV: &[u8] = b"RIFF\x08\x00\x00\x00WAVE";
const TINY_GIF: &[u8] = b"GIF89a";
const UNKNOWN_BLOB: &str =
    "sha256-abababababababababababababababababababababababababababababababab";
const CLIP_ID: &str = "clip-alpha";
const OVERLAY_ID: &str = "alert-box";
const SOURCE_SLOT: &str = "source";
const SOUND_SLOT: &str = "sound";
const DATABASE_FILE: &str = "forge.db";
const MEDIA_DIR: &str = "media";
const STARTUP_SWEEP_DEADLINE: Duration = Duration::from_secs(30);
const STARTUP_SWEEP_POLL: Duration = Duration::from_millis(10);

struct Fixture {
    backend: SqliteBackend,
    dir: TempDir,
}

async fn open_at(dir: &TempDir) -> SqliteBackend {
    let url = common::sqlite_url(&dir.path().join(DATABASE_FILE));
    SqliteBackend::open_for_test(&url, TEST_KEY, dir.path().join(MEDIA_DIR), None)
        .await
        .expect("a file-backed backend over the fixture directory")
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("temp dir");
    let backend = open_at(&dir).await;
    Fixture { backend, dir }
}

impl Fixture {
    fn media(&self) -> Arc<dyn MediaRepo> {
        self.backend.media_repo()
    }

    async fn stored(&self, label: &str, bytes: &[u8]) -> (MediaBlobId, PathBuf) {
        let blob = self
            .media()
            .store(label, bytes.to_vec())
            .await
            .expect("store");
        let path = self.media().resolve(&blob.id).await.expect("resolve");
        (blob.id, path)
    }

    async fn retained(&self, referrer: &MediaReferrer, label: &str, bytes: &[u8]) -> PathBuf {
        let (id, path) = self.stored(label, bytes).await;
        self.media().retain(referrer, &id).await.expect("retain");
        path
    }

    async fn reopened(self) -> Self {
        self.backend.shutdown().await;
        drop(self.backend);
        let backend = open_at(&self.dir).await;
        Self {
            backend,
            dir: self.dir,
        }
    }

    async fn kept(&self, id: &MediaBlobId, path: &std::path::Path) -> (bool, bool) {
        (
            self.media().get(id).await.expect("get").is_some(),
            path.exists(),
        )
    }
}

fn blob_id_at(path: &std::path::Path) -> MediaBlobId {
    let stem = path
        .file_stem()
        .expect("a managed file has a stem")
        .to_string_lossy();
    MediaBlobId::from_stored(stem.into_owned()).expect("managed files are named after their blob")
}

fn clip_source() -> MediaReferrer {
    MediaReferrer::new(MediaReferrerKind::SoundboardClip, CLIP_ID, SOURCE_SLOT)
}

fn overlay_sound() -> MediaReferrer {
    MediaReferrer::new(MediaReferrerKind::Overlay, OVERLAY_ID, SOUND_SLOT)
}

#[derive(Debug, Clone, Copy)]
enum ReleaseRoute {
    Release,
    ReleaseAll,
    BatchRelease,
    BatchReleaseAll,
}

async fn release_clip_source(fx: &Fixture, route: ReleaseRoute) {
    let referrer = clip_source();
    match route {
        ReleaseRoute::Release => {
            fx.media().release(&referrer).await.expect("release");
        }
        ReleaseRoute::ReleaseAll => {
            fx.media()
                .release_all(referrer.kind, &referrer.id)
                .await
                .expect("release_all");
        }
        ReleaseRoute::BatchRelease => fx
            .backend
            .commit_changes(&[StoreChange::ReleaseMedia(referrer)])
            .await
            .expect("batch release"),
        ReleaseRoute::BatchReleaseAll => fx
            .backend
            .commit_changes(&[StoreChange::ReleaseAllMedia {
                kind: referrer.kind,
                referrer_id: referrer.id,
            }])
            .await
            .expect("batch release_all"),
    }
}

#[tokio::test]
async fn the_last_release_removes_the_audio_row_and_file_whichever_route_releases_it() {
    for route in [
        ReleaseRoute::Release,
        ReleaseRoute::ReleaseAll,
        ReleaseRoute::BatchRelease,
        ReleaseRoute::BatchReleaseAll,
    ] {
        let fx = fixture().await;
        let path = fx.retained(&clip_source(), "fanfare.wav", TINY_WAV).await;

        release_clip_source(&fx, route).await;

        assert_eq!(
            fx.kept(&blob_id_at(&path), &path).await,
            (false, false),
            "{route:?} left the unheld audio behind"
        );
    }
}

#[tokio::test]
async fn audio_another_referrer_still_holds_survives_the_release() {
    let fx = fixture().await;
    let path = fx.retained(&clip_source(), "fanfare.wav", TINY_WAV).await;
    let id = blob_id_at(&path);
    fx.media()
        .retain(&overlay_sound(), &id)
        .await
        .expect("retain by the overlay");

    release_clip_source(&fx, ReleaseRoute::Release).await;

    assert_eq!(fx.kept(&id, &path).await, (true, true));
}

#[tokio::test]
async fn replacing_the_audio_in_a_slot_removes_the_audio_it_held() {
    for batched in [false, true] {
        let fx = fixture().await;
        let old_path = fx.retained(&clip_source(), "old.wav", TINY_WAV).await;
        let (new_id, _) = fx.stored("new.wav", OTHER_WAV).await;

        if batched {
            fx.backend
                .commit_changes(&[StoreChange::RetainMedia {
                    referrer: clip_source(),
                    blob: new_id,
                }])
                .await
                .expect("batch retain");
        } else {
            fx.media()
                .retain(&clip_source(), &new_id)
                .await
                .expect("retain");
        }

        assert_eq!(
            fx.kept(&blob_id_at(&old_path), &old_path).await,
            (false, false),
            "batched {batched}: the replaced audio was left behind"
        );
    }
}

#[tokio::test]
async fn releasing_and_retaining_the_same_audio_in_one_batch_keeps_it() {
    let fx = fixture().await;
    let path = fx.retained(&clip_source(), "fanfare.wav", TINY_WAV).await;
    let id = blob_id_at(&path);

    fx.backend
        .commit_changes(&[
            StoreChange::ReleaseMedia(clip_source()),
            StoreChange::RetainMedia {
                referrer: overlay_sound(),
                blob: id.clone(),
            },
        ])
        .await
        .expect("batch");

    assert_eq!(fx.kept(&id, &path).await, (true, true));
}

#[tokio::test]
async fn a_batch_failing_after_the_last_release_keeps_the_audio_and_its_holder() {
    let fx = fixture().await;
    let path = fx.retained(&clip_source(), "fanfare.wav", TINY_WAV).await;
    let id = blob_id_at(&path);

    let error = fx
        .backend
        .commit_changes(&[
            StoreChange::ReleaseMedia(clip_source()),
            StoreChange::RetainMedia {
                referrer: overlay_sound(),
                blob: MediaBlobId::from_stored(UNKNOWN_BLOB).expect("well formed blob id"),
            },
        ])
        .await
        .unwrap_err();

    assert!(matches!(error, StorageError::NotFound { .. }), "{error:?}");
    assert_eq!(
        (
            fx.kept(&id, &path).await,
            fx.media().blob_of(&clip_source()).await.expect("blob_of"),
        ),
        ((true, true), Some(id))
    );
}

#[tokio::test]
async fn an_image_released_by_its_last_user_stays_in_the_library() {
    for route in [ReleaseRoute::Release, ReleaseRoute::BatchReleaseAll] {
        let fx = fixture().await;
        let path = fx.retained(&clip_source(), "badge.gif", TINY_GIF).await;

        release_clip_source(&fx, route).await;

        assert_eq!(
            fx.kept(&blob_id_at(&path), &path).await,
            (true, true),
            "{route:?} collected a library image"
        );
    }
}

#[tokio::test]
async fn deleting_an_image_only_the_library_holds_removes_its_row_and_file() {
    let fx = fixture().await;
    let path = fx.retained(&clip_source(), "badge.gif", TINY_GIF).await;
    let id = blob_id_at(&path);
    release_clip_source(&fx, ReleaseRoute::Release).await;

    let deleted = fx.media().delete(&id).await.expect("delete");

    assert_eq!((deleted, fx.kept(&id, &path).await), (true, (false, false)));
}

#[tokio::test]
async fn reopening_removes_audio_nothing_holds_with_its_file() {
    let fx = fixture().await;
    let (id, path) = fx.stored("stranded.wav", TINY_WAV).await;

    let fx = fx.reopened().await;

    assert!(fx.media().get(&id).await.expect("get").is_none());
    tokio::time::timeout(STARTUP_SWEEP_DEADLINE, async {
        while path.exists() {
            tokio::time::sleep(STARTUP_SWEEP_POLL).await;
        }
    })
    .await
    .expect("the startup sweep never removed the unreferenced file");
}

#[tokio::test]
async fn reopening_keeps_held_audio_and_library_images() {
    let fx = fixture().await;
    let (_, stranded) = fx.stored("stranded.wav", TINY_WAV).await;
    let held = fx.retained(&clip_source(), "fanfare.wav", OTHER_WAV).await;
    let (image, image_path) = fx.stored("badge.gif", TINY_GIF).await;

    let fx = fx.reopened().await;
    tokio::time::timeout(STARTUP_SWEEP_DEADLINE, async {
        while stranded.exists() {
            tokio::time::sleep(STARTUP_SWEEP_POLL).await;
        }
    })
    .await
    .expect("the startup sweep never finished");

    assert_eq!(
        (
            fx.kept(&blob_id_at(&held), &held).await,
            fx.kept(&image, &image_path).await,
        ),
        ((true, true), (true, true))
    );
}
