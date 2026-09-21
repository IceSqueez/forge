#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_soundboard::{ClipAvailability, ClipLibrary, ClipSource, SoundboardError};
use forge_storage::{
    MediaBlob, MediaBlobId, MediaFormat, MediaReferrer, MediaReferrerKind, MediaRepo,
    SoundboardClipsRepo, StorageError, StoredClip,
};
use forge_types::{ClipId, OutputDevice};
use time::OffsetDateTime;
use tokio::sync::Notify;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

const CLIP_SOURCE_SLOT: &str = "source";
const FANFARE: &str = "/home/streamer/sounds/fanfare.wav";
const AIRHORN: &str = "/home/streamer/sounds/airhorn.wav";
const UNREADABLE: &str = "/home/streamer/sounds/notes.txt";
const FANFARE_BLOB: &str = "sha256-fanfare";
const AIRHORN_BLOB: &str = "sha256-airhorn";
const MANAGED_FANFARE: &str = "/data/media/sha256-fanfare.wav";
const MANAGED_AIRHORN: &str = "/data/media/sha256-airhorn.wav";

#[derive(Debug, PartialEq, Eq)]
enum MediaCall {
    Import(PathBuf),
    Retain(MediaReferrer, MediaBlobId),
    ReleaseAll(MediaReferrerKind, String),
    Delete(MediaBlobId),
}

#[derive(Default)]
struct MediaState {
    known: HashMap<PathBuf, MediaBlobId>,
    resolvable: HashMap<MediaBlobId, PathBuf>,
    retained: HashMap<(String, String, String), MediaBlobId>,
}

struct FakeMedia {
    state: Mutex<MediaState>,
    calls: UnboundedSender<MediaCall>,
    gate: Option<Arc<Notify>>,
}

impl FakeMedia {
    fn new(gate: Option<Arc<Notify>>) -> (Arc<Self>, UnboundedReceiver<MediaCall>) {
        let (calls, rx) = unbounded_channel();
        (
            Arc::new(Self {
                state: Mutex::new(MediaState::default()),
                calls,
                gate,
            }),
            rx,
        )
    }

    fn knows(&self, source: &str, blob: &str, managed: &str) {
        let id = MediaBlobId::from_stored(blob);
        let mut state = self.state.lock().unwrap();
        state.known.insert(PathBuf::from(source), id.clone());
        state.resolvable.insert(id, PathBuf::from(managed));
    }

    fn forget_file(&self, blob: &str) {
        self.state
            .lock()
            .unwrap()
            .resolvable
            .remove(&MediaBlobId::from_stored(blob));
    }

    fn already_retained(&self, clip_id: ClipId, blob: &str) {
        self.state.lock().unwrap().retained.insert(
            slot_key(&clip_source(clip_id)),
            MediaBlobId::from_stored(blob),
        );
    }
}

fn slot_key(referrer: &MediaReferrer) -> (String, String, String) {
    (
        referrer.kind.as_str().to_owned(),
        referrer.id.clone(),
        referrer.slot.clone(),
    )
}

fn clip_source(clip_id: ClipId) -> MediaReferrer {
    MediaReferrer::new(
        MediaReferrerKind::SoundboardClip,
        clip_id.to_string(),
        CLIP_SOURCE_SLOT,
    )
}

#[async_trait]
impl MediaRepo for FakeMedia {
    async fn store(&self, _label: &str, _bytes: Vec<u8>) -> Result<MediaBlob, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn import_file(&self, source: &Path) -> Result<MediaBlob, StorageError> {
        let _ = self.calls.send(MediaCall::Import(source.to_path_buf()));
        if let Some(gate) = &self.gate {
            gate.notified().await;
        }
        let known = self.state.lock().unwrap().known.get(source).cloned();
        match known {
            Some(id) => Ok(MediaBlob {
                id,
                format: MediaFormat::Wav,
                byte_size: 12,
                label: "fixture.wav".to_owned(),
                imported_at: OffsetDateTime::UNIX_EPOCH,
            }),
            None => Err(StorageError::MediaUnsupported {
                label: source
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            }),
        }
    }

    async fn get(&self, _id: &MediaBlobId) -> Result<Option<MediaBlob>, StorageError> {
        Ok(None)
    }

    async fn list(&self) -> Result<Vec<MediaBlob>, StorageError> {
        Ok(Vec::new())
    }

    async fn total_bytes(&self) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn resolve(&self, id: &MediaBlobId) -> Result<PathBuf, StorageError> {
        self.state
            .lock()
            .unwrap()
            .resolvable
            .get(id)
            .cloned()
            .ok_or_else(|| StorageError::NotFound {
                key: id.as_str().to_owned(),
            })
    }

    async fn read(&self, _id: &MediaBlobId) -> Result<Vec<u8>, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn delete(&self, id: &MediaBlobId) -> Result<bool, StorageError> {
        let _ = self.calls.send(MediaCall::Delete(id.clone()));
        Ok(true)
    }

    async fn retain(&self, referrer: &MediaReferrer, id: &MediaBlobId) -> Result<(), StorageError> {
        let _ = self
            .calls
            .send(MediaCall::Retain(referrer.clone(), id.clone()));
        self.state
            .lock()
            .unwrap()
            .retained
            .insert(slot_key(referrer), id.clone());
        Ok(())
    }

    async fn release(&self, referrer: &MediaReferrer) -> Result<bool, StorageError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .retained
            .remove(&slot_key(referrer))
            .is_some())
    }

    async fn release_all(
        &self,
        kind: MediaReferrerKind,
        referrer_id: &str,
    ) -> Result<u64, StorageError> {
        let _ = self
            .calls
            .send(MediaCall::ReleaseAll(kind, referrer_id.to_owned()));
        let mut state = self.state.lock().unwrap();
        let doomed: Vec<_> = state
            .retained
            .keys()
            .filter(|(row_kind, row_id, _)| row_kind == kind.as_str() && row_id == referrer_id)
            .cloned()
            .collect();
        for key in &doomed {
            state.retained.remove(key);
        }
        Ok(doomed.len() as u64)
    }

    async fn referrers(&self, _id: &MediaBlobId) -> Result<Vec<MediaReferrer>, StorageError> {
        Ok(Vec::new())
    }

    async fn blob_of(&self, referrer: &MediaReferrer) -> Result<Option<MediaBlobId>, StorageError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .retained
            .get(&slot_key(referrer))
            .cloned())
    }
}

#[derive(Default)]
struct FakeClips {
    rows: Mutex<HashMap<ClipId, StoredClip>>,
    saved: Mutex<Vec<ClipId>>,
}

#[async_trait]
impl SoundboardClipsRepo for FakeClips {
    async fn list(&self) -> Result<Vec<StoredClip>, StorageError> {
        let mut rows: Vec<StoredClip> = self.rows.lock().unwrap().values().cloned().collect();
        rows.sort_by_key(|row| row.name.clone());
        Ok(rows)
    }

    async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, StorageError> {
        Ok(self.rows.lock().unwrap().get(&id).cloned())
    }

    async fn save(&self, clip: &StoredClip) -> Result<(), StorageError> {
        self.saved.lock().unwrap().push(clip.id);
        self.rows.lock().unwrap().insert(clip.id, clip.clone());
        Ok(())
    }

    async fn delete(&self, id: ClipId) -> Result<bool, StorageError> {
        Ok(self.rows.lock().unwrap().remove(&id).is_some())
    }
}

struct Harness {
    library: Arc<ClipLibrary>,
    clips: Arc<FakeClips>,
    media: Arc<FakeMedia>,
    calls: UnboundedReceiver<MediaCall>,
}

fn harness() -> Harness {
    harness_with_gate(None)
}

fn harness_with_gate(gate: Option<Arc<Notify>>) -> Harness {
    let clips = Arc::new(FakeClips::default());
    let (media, calls) = FakeMedia::new(gate);
    let library = Arc::new(ClipLibrary::new(
        Arc::clone(&clips) as Arc<dyn SoundboardClipsRepo>,
        Arc::clone(&media) as Arc<dyn MediaRepo>,
    ));
    Harness {
        library,
        clips,
        media,
        calls,
    }
}

fn clip(name: &str, source: &str) -> StoredClip {
    StoredClip {
        id: ClipId::new(),
        name: name.to_owned(),
        file_path: PathBuf::from(source),
        volume: 1.0,
        output_device: OutputDevice::Default,
        hotkey: None,
        created_at: OffsetDateTime::UNIX_EPOCH,
        category: String::new(),
        loop_playback: false,
        duration_secs: None,
        builtin_id: None,
    }
}

fn drain(calls: &mut UnboundedReceiver<MediaCall>) -> Vec<MediaCall> {
    let mut seen = Vec::new();
    while let Ok(call) = calls.try_recv() {
        seen.push(call);
    }
    seen
}

#[tokio::test]
async fn a_refused_import_on_save_writes_no_clip_row() {
    let mut fx = harness();
    let row = clip("Notes", UNREADABLE);

    let error = fx.library.save_clip(&row).await.unwrap_err();

    assert!(
        matches!(
            error,
            SoundboardError::ImportRefused(StorageError::MediaUnsupported { .. })
        ),
        "an unsupported source was not reported as a refusal: {error:?}"
    );
    assert!(fx.clips.saved.lock().unwrap().is_empty());
    assert_eq!(
        drain(&mut fx.calls),
        vec![MediaCall::Import(PathBuf::from(UNREADABLE))]
    );
}

#[tokio::test]
async fn saving_a_clip_with_a_changed_path_retains_the_blob_of_the_new_file() {
    let mut fx = harness();
    fx.media.knows(FANFARE, FANFARE_BLOB, MANAGED_FANFARE);
    fx.media.knows(AIRHORN, AIRHORN_BLOB, MANAGED_AIRHORN);

    let mut row = clip("Alert", FANFARE);
    fx.library.save_clip(&row).await.expect("first save");
    let _ = drain(&mut fx.calls);

    row.file_path = PathBuf::from(AIRHORN);
    fx.library.save_clip(&row).await.expect("second save");

    assert_eq!(
        drain(&mut fx.calls),
        vec![
            MediaCall::Import(PathBuf::from(AIRHORN)),
            MediaCall::Retain(clip_source(row.id), MediaBlobId::from_stored(AIRHORN_BLOB)),
        ]
    );
}

#[tokio::test]
async fn saving_an_untouched_clip_that_is_already_managed_imports_nothing() {
    let mut fx = harness();
    fx.media.knows(FANFARE, FANFARE_BLOB, MANAGED_FANFARE);
    let row = clip("Alert", FANFARE);
    fx.library.save_clip(&row).await.expect("first save");
    let _ = drain(&mut fx.calls);

    fx.library.save_clip(&row).await.expect("second save");

    assert!(drain(&mut fx.calls).is_empty());
}

#[tokio::test]
async fn deleting_a_clip_releases_its_reference_and_leaves_the_blob_alone() {
    let mut fx = harness();
    fx.media.knows(FANFARE, FANFARE_BLOB, MANAGED_FANFARE);
    let row = clip("Alert", FANFARE);
    fx.library.save_clip(&row).await.expect("save");
    let _ = drain(&mut fx.calls);

    assert!(fx.library.delete_clip(row.id).await.expect("delete"));

    assert_eq!(
        drain(&mut fx.calls),
        vec![MediaCall::ReleaseAll(
            MediaReferrerKind::SoundboardClip,
            row.id.to_string()
        )]
    );
}

#[tokio::test]
async fn repeated_triggers_import_a_clip_once_while_its_adoption_is_in_flight() {
    const TRIGGERS: usize = 5;
    let gate = Arc::new(Notify::new());
    let mut fx = harness_with_gate(Some(Arc::clone(&gate)));
    fx.media.knows(FANFARE, FANFARE_BLOB, MANAGED_FANFARE);
    let row = clip("Alert", FANFARE);

    for _ in 0..TRIGGERS {
        fx.library
            .adopt_in_background(row.id, PathBuf::from(FANFARE));
    }

    assert_eq!(
        fx.calls.recv().await,
        Some(MediaCall::Import(PathBuf::from(FANFARE)))
    );
    gate.notify_one();

    assert_eq!(
        fx.calls.recv().await,
        Some(MediaCall::Retain(
            clip_source(row.id),
            MediaBlobId::from_stored(FANFARE_BLOB)
        ))
    );
    assert!(drain(&mut fx.calls).is_empty());
}

#[tokio::test]
async fn an_adoption_refused_as_unsupported_is_never_retried() {
    let mut fx = harness();
    let row = clip("Notes", UNREADABLE);
    fx.clips.save(&row).await.expect("seed the row");

    fx.library
        .save_clip(&row)
        .await
        .expect("a refused adoption must not fail the save");
    assert_eq!(
        drain(&mut fx.calls),
        vec![MediaCall::Import(PathBuf::from(UNREADABLE))]
    );

    fx.library
        .adopt_in_background(row.id, PathBuf::from(UNREADABLE));

    assert!(drain(&mut fx.calls).is_empty());
}

#[tokio::test]
async fn a_managed_blob_whose_file_vanished_falls_back_to_the_legacy_path() {
    let fx = harness();
    let legacy = tempfile::NamedTempFile::new().expect("legacy stand-in");
    let legacy_path = legacy.path().to_path_buf();
    let row = clip("Alert", &legacy_path.to_string_lossy());
    fx.media.already_retained(row.id, FANFARE_BLOB);
    fx.media.forget_file(FANFARE_BLOB);

    let source = fx.library.source_of(&row).await;

    assert_eq!(source, ClipSource::Legacy(legacy_path));
}

#[tokio::test]
async fn source_of_reports_missing_when_no_copy_of_the_clip_resolves() {
    let fx = harness();
    let row = clip("Alert", FANFARE);

    assert_eq!(fx.library.source_of(&row).await, ClipSource::Missing);
}

#[tokio::test]
async fn availability_of_reports_each_clip_in_a_mixed_list() {
    let fx = harness();
    let managed_source = tempfile::NamedTempFile::new().expect("managed stand-in");
    let legacy_source = tempfile::NamedTempFile::new().expect("legacy stand-in");

    let managed = clip("Managed", &managed_source.path().to_string_lossy());
    let legacy = clip("Legacy", &legacy_source.path().to_string_lossy());
    let gone = clip("Gone", FANFARE);

    fx.media.knows(
        &managed_source.path().to_string_lossy(),
        FANFARE_BLOB,
        &managed_source.path().to_string_lossy(),
    );
    fx.media.already_retained(managed.id, FANFARE_BLOB);

    let states: HashMap<ClipId, ClipAvailability> = fx
        .library
        .availability_of(&[managed.clone(), legacy.clone(), gone.clone()])
        .await
        .into_iter()
        .collect();

    assert_eq!(
        states,
        HashMap::from([
            (managed.id, ClipAvailability::Managed),
            (legacy.id, ClipAvailability::Unadopted { refusal: None }),
            (gone.id, ClipAvailability::Missing),
        ])
    );
}

#[tokio::test]
async fn availability_of_returns_nothing_for_an_empty_list() {
    let fx = harness();

    assert!(fx.library.availability_of(&[]).await.is_empty());
}
