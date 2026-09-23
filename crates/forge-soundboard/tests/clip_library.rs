#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventSource};
use forge_soundboard::{
    AdoptionVerdict, ClipAvailability, ClipLibrary, ClipRefusal, ClipSource, SoundboardError,
};
use forge_storage::{
    MediaBlob, MediaBlobId, MediaFormat, MediaReferrer, MediaReferrerKind, MediaRepo,
    SoundboardClipsRepo, StorageError, StoredClip,
};
use forge_types::{ClipId, OutputDevice};
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio::sync::Notify;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

const SETTLE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
const ADOPTION_EVENT: &str = "soundboard.clip.adopted";
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
    flaky: HashSet<PathBuf>,
    retain_fails: bool,
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

    fn stumbles_on(&self, source: &Path) {
        self.state
            .lock()
            .unwrap()
            .flaky
            .insert(source.to_path_buf());
    }

    fn breaks_retain(&self) {
        self.state.lock().unwrap().retain_fails = true;
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
        let (known, flaky) = {
            let state = self.state.lock().unwrap();
            (
                state.known.get(source).cloned(),
                state.flaky.contains(source),
            )
        };
        if flaky {
            return Err(StorageError::Connection {
                reason: "the media store is busy".to_owned(),
            });
        }
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
        let mut state = self.state.lock().unwrap();
        if state.retain_fails {
            return Err(StorageError::Connection {
                reason: "the reference table is locked".to_owned(),
            });
        }
        state.retained.insert(slot_key(referrer), id.clone());
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

/// Captures everything the library announces so a test can assert both WHICH events fired and
/// that no extra one did.
#[derive(Default)]
struct RecordingPublisher {
    events: Mutex<Vec<Event>>,
}

impl EventPublisher for RecordingPublisher {
    fn publish(&self, event: Event) {
        self.events.lock().unwrap().push(event);
    }
}

impl RecordingPublisher {
    /// Payloads of the adoption announcements, in the order they were published.
    fn settled(&self) -> Vec<Value> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.source == EventSource::Audio && event.kind == ADOPTION_EVENT)
            .map(|event| event.payload.clone())
            .collect()
    }

    fn published(&self) -> usize {
        self.events.lock().unwrap().len()
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

impl Harness {
    fn listen(&self) -> Arc<RecordingPublisher> {
        let publisher = Arc::new(RecordingPublisher::default());
        self.library
            .install_event_publisher(Arc::clone(&publisher) as Arc<dyn EventPublisher>);
        publisher
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

fn legacy_source(suffix: &str) -> tempfile::NamedTempFile {
    tempfile::Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("legacy stand-in")
}

fn clip_at(name: &str, source: &Path) -> StoredClip {
    clip(name, &source.to_string_lossy())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .expect("a named source")
        .to_string_lossy()
        .into_owned()
}

async fn settle_background_work() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
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
async fn a_refusal_recorded_while_saving_explains_the_row_in_a_later_availability_sweep() {
    let mut fx = harness();
    let source = legacy_source(".txt");
    let row = clip_at("Notes", source.path());
    fx.clips.save(&row).await.expect("seed the row");

    fx.library
        .save_clip(&row)
        .await
        .expect("a refused adoption must not fail the save");
    let _ = drain(&mut fx.calls);

    assert_eq!(
        fx.library.availability_of(std::slice::from_ref(&row)).await,
        vec![(
            row.id,
            ClipAvailability::Unadopted {
                refusal: Some(ClipRefusal::Unsupported {
                    label: file_name(source.path())
                })
            }
        )]
    );
}

#[tokio::test]
async fn a_recorded_refusal_stops_the_background_path_but_not_an_explicit_adoption() {
    let mut fx = harness();
    let source = legacy_source(".txt");
    let row = clip_at("Notes", source.path());

    let refused = fx
        .library
        .adopt_now(&row)
        .await
        .expect("an unsupported source settles with a verdict");
    assert!(
        matches!(
            refused,
            AdoptionVerdict::Refused(ClipRefusal::Unsupported { .. })
        ),
        "an unsupported source was not refused: {refused:?}"
    );
    let _ = drain(&mut fx.calls);

    fx.media.knows(
        &source.path().to_string_lossy(),
        FANFARE_BLOB,
        MANAGED_FANFARE,
    );
    fx.library
        .adopt_in_background(row.id, source.path().to_path_buf());
    settle_background_work().await;
    assert!(
        drain(&mut fx.calls).is_empty(),
        "the background path retried a source it had already refused"
    );

    assert_eq!(
        fx.library.adopt_now(&row).await.expect("the retry settles"),
        AdoptionVerdict::Adopted
    );
}

#[tokio::test]
async fn an_adoption_already_running_reports_in_flight_instead_of_importing_again() {
    let gate = Arc::new(Notify::new());
    let mut fx = harness_with_gate(Some(Arc::clone(&gate)));
    let source = legacy_source(".wav");
    fx.media.knows(
        &source.path().to_string_lossy(),
        FANFARE_BLOB,
        MANAGED_FANFARE,
    );
    let row = clip_at("Alert", source.path());

    fx.library
        .adopt_in_background(row.id, source.path().to_path_buf());
    assert_eq!(
        fx.calls.recv().await,
        Some(MediaCall::Import(source.path().to_path_buf()))
    );

    let verdict = tokio::time::timeout(SETTLE_DEADLINE, fx.library.adopt_now(&row))
        .await
        .expect("the second caller must not wait on the running import")
        .expect("the second caller settles");

    assert_eq!(verdict, AdoptionVerdict::InFlight);
}

#[tokio::test]
async fn an_adoption_whose_reference_write_fails_is_not_reported_as_copied() {
    let fx = harness();
    let source = legacy_source(".wav");
    fx.media.knows(
        &source.path().to_string_lossy(),
        FANFARE_BLOB,
        MANAGED_FANFARE,
    );
    fx.media.breaks_retain();
    let row = clip_at("Alert", source.path());

    let error = fx.library.adopt_now(&row).await.unwrap_err();

    assert!(
        matches!(error, SoundboardError::Storage(_)),
        "a lost reference write was not reported: {error:?}"
    );
}

#[tokio::test]
async fn saving_a_chosen_source_whose_reference_write_fails_reports_the_failure() {
    let fx = harness();
    let source = legacy_source(".wav");
    fx.media.knows(
        &source.path().to_string_lossy(),
        FANFARE_BLOB,
        MANAGED_FANFARE,
    );
    fx.media.breaks_retain();
    let row = clip_at("Alert", source.path());

    let error = fx.library.save_clip(&row).await.unwrap_err();

    assert!(
        matches!(error, SoundboardError::Storage(_)),
        "a lost reference write was not reported: {error:?}"
    );
}

#[tokio::test]
async fn a_transient_import_failure_leaves_the_row_open_to_another_attempt() {
    let fx = harness();
    let source = legacy_source(".wav");
    fx.media.stumbles_on(source.path());
    let row = clip_at("Alert", source.path());

    let error = fx.library.adopt_now(&row).await.unwrap_err();
    assert!(
        matches!(error, SoundboardError::Storage(_)),
        "a busy store was not reported as a storage failure: {error:?}"
    );

    assert_eq!(
        fx.library.availability_of(std::slice::from_ref(&row)).await,
        vec![(row.id, ClipAvailability::Unadopted { refusal: None })]
    );
}

#[tokio::test]
async fn adopt_all_now_settles_every_row_with_its_own_verdict() {
    let fx = harness();
    let managed_source = legacy_source(".wav");
    let adoptable_source = legacy_source(".wav");
    let refused_source = legacy_source(".txt");

    let managed = clip_at("Managed", managed_source.path());
    fx.media.knows(
        &managed_source.path().to_string_lossy(),
        FANFARE_BLOB,
        &managed_source.path().to_string_lossy(),
    );
    fx.media.already_retained(managed.id, FANFARE_BLOB);

    let adoptable = clip_at("Adoptable", adoptable_source.path());
    fx.media.knows(
        &adoptable_source.path().to_string_lossy(),
        AIRHORN_BLOB,
        MANAGED_AIRHORN,
    );

    let refused = clip_at("Refused", refused_source.path());
    let gone = clip("Gone", FANFARE);

    let verdicts: HashMap<ClipId, AdoptionVerdict> = fx
        .library
        .adopt_all_now(&[
            managed.clone(),
            adoptable.clone(),
            refused.clone(),
            gone.clone(),
        ])
        .await
        .into_iter()
        .map(|(id, verdict)| (id, verdict.expect("every row settles with a verdict")))
        .collect();

    assert_eq!(
        verdicts,
        HashMap::from([
            (managed.id, AdoptionVerdict::AlreadyManaged),
            (adoptable.id, AdoptionVerdict::Adopted),
            (
                refused.id,
                AdoptionVerdict::Refused(ClipRefusal::Unsupported {
                    label: file_name(refused_source.path())
                })
            ),
            (gone.id, AdoptionVerdict::SourceMissing),
        ])
    );
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

#[tokio::test]
async fn adopting_a_list_announces_one_settled_event_per_row_carrying_its_verdict() {
    let fx = harness();
    let bus = fx.listen();
    let managed_source = legacy_source(".wav");
    let adoptable_source = legacy_source(".wav");
    let refused_source = legacy_source(".txt");

    let managed = clip_at("Managed", managed_source.path());
    fx.media.knows(
        &managed_source.path().to_string_lossy(),
        FANFARE_BLOB,
        &managed_source.path().to_string_lossy(),
    );
    fx.media.already_retained(managed.id, FANFARE_BLOB);

    let adoptable = clip_at("Adoptable", adoptable_source.path());
    fx.media.knows(
        &adoptable_source.path().to_string_lossy(),
        AIRHORN_BLOB,
        MANAGED_AIRHORN,
    );

    let refused = clip_at("Refused", refused_source.path());
    let gone = clip("Gone", FANFARE);

    fx.library
        .adopt_all_now(&[
            managed.clone(),
            adoptable.clone(),
            refused.clone(),
            gone.clone(),
        ])
        .await;

    let announced = bus.settled();
    assert_eq!(
        announced,
        vec![
            json!({ "clip_id": managed.id.to_string(), "verdict": "already_managed" }),
            json!({ "clip_id": adoptable.id.to_string(), "verdict": "adopted" }),
            json!({
                "clip_id": refused.id.to_string(),
                "verdict": "refused",
                "reason": ClipRefusal::Unsupported {
                    label: file_name(refused_source.path()),
                }
                .to_string(),
            }),
            json!({ "clip_id": gone.id.to_string(), "verdict": "source_missing" }),
        ],
    );
    assert_eq!(
        bus.published(),
        announced.len(),
        "the library published something other than an adoption verdict",
    );
}

#[tokio::test]
async fn a_refused_adoption_names_the_reason_without_leaking_the_source_path() {
    let fx = harness();
    let bus = fx.listen();
    let source = legacy_source(".txt");
    let row = clip_at("Notes", source.path());

    fx.library
        .adopt_now(&row)
        .await
        .expect("an unsupported source settles with a verdict");

    let announced = bus.settled();
    let [payload] = announced.as_slice() else {
        panic!("expected exactly one settled event, got {announced:?}");
    };
    let text = payload.to_string();
    let directory = source.path().parent().expect("a temp file has a parent");
    assert!(
        !text.contains(&*source.path().to_string_lossy()),
        "the refusal event carried the full source path: {text}",
    );
    assert!(
        !text.contains(&*directory.to_string_lossy()),
        "the refusal event carried the source directory: {text}",
    );
}

#[tokio::test]
async fn an_adoption_already_running_announces_nothing_for_the_second_caller() {
    let gate = Arc::new(Notify::new());
    let mut fx = harness_with_gate(Some(Arc::clone(&gate)));
    let bus = fx.listen();
    let source = legacy_source(".wav");
    fx.media.knows(
        &source.path().to_string_lossy(),
        FANFARE_BLOB,
        MANAGED_FANFARE,
    );
    let row = clip_at("Alert", source.path());

    fx.library
        .adopt_in_background(row.id, source.path().to_path_buf());
    assert_eq!(
        fx.calls.recv().await,
        Some(MediaCall::Import(source.path().to_path_buf()))
    );

    let verdict = tokio::time::timeout(SETTLE_DEADLINE, fx.library.adopt_now(&row))
        .await
        .expect("the second caller must not wait on the running import")
        .expect("the second caller settles");
    assert_eq!(verdict, AdoptionVerdict::InFlight);

    assert!(
        bus.settled().is_empty(),
        "an unsettled adoption was announced: {:?}",
        bus.settled(),
    );
}

#[tokio::test]
async fn a_recorded_refusal_silences_the_background_path_but_not_an_explicit_retry() {
    let fx = harness();
    let bus = fx.listen();
    let source = legacy_source(".txt");
    let row = clip_at("Notes", source.path());

    fx.library
        .adopt_now(&row)
        .await
        .expect("the first attempt settles");
    assert_eq!(bus.settled().len(), 1, "the refusal was not announced");

    fx.library
        .adopt_in_background(row.id, source.path().to_path_buf());
    settle_background_work().await;
    assert_eq!(
        bus.settled().len(),
        1,
        "the background path announced a refusal it never retried",
    );

    fx.library
        .adopt_now(&row)
        .await
        .expect("the explicit retry settles");
    assert_eq!(
        bus.settled().len(),
        2,
        "the explicit retry settled without announcing it",
    );
}

#[tokio::test]
async fn an_adoption_settles_normally_while_no_publisher_is_installed() {
    let fx = harness();
    let source = legacy_source(".wav");
    fx.media.knows(
        &source.path().to_string_lossy(),
        FANFARE_BLOB,
        MANAGED_FANFARE,
    );
    let row = clip_at("Alert", source.path());

    assert_eq!(
        fx.library
            .adopt_now(&row)
            .await
            .expect("the adoption settles without a bus to announce on"),
        AdoptionVerdict::Adopted
    );
}
