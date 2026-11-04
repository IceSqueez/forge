#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test fixtures"
)]
mod common;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_overlay::config::HEADLINE;
use forge_overlay::motion::{EXIT, NO_MOTION};
use forge_overlay::{
    SHOW_IMAGE, SHOW_MEDIA_DIRECTORY, SHOW_SOUND, clip_reference, image_reference,
};
use forge_runtime::{
    EventBus, NullEventLogRepo, OverlayDispatch, OverlayFrameSink, OverlayMediaLibrary,
    OverlayReceivers, OverlaySendOptions, OverlayServiceHandle, OverlayServiceParts, ShowEnd,
};
use forge_storage::settings::core_settings;
use forge_storage::test_support::MemorySettings;
use forge_storage::{
    MediaBlob, MediaBlobId, MediaFormat, MediaReferrerKind, MediaRepo, MockMediaRepo,
    MockOverlayRepo, MockSoundboardClipsRepo, OverlayConfig, OverlayId, OverlayRepo, SettingsRepo,
    SoundboardClipsRepo, StoredClip,
};
use forge_types::{ArgStack, ClipId, OutputDevice, Variant};
use serde_json::Value;
use tempfile::TempDir;
use time::OffsetDateTime;

const ALERT_KIND: &str = "overlay.alert";
const FRAME_KIND: &str = "overlay.frame";
const ALERT: &str = "stage-alert";
const FRAME: &str = "stage-frame";
const CLIP_ULID: &str = "01J9P4S2M7Q8V3X5Y6Z7A8B9C0";
const UNKNOWN_CLIP_ULID: &str = "01J9P4S2M7Q8V3X5Y6Z7A8B9C1";
const CLIP_BLOB: &str = "sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8";
const IMAGE_BLOB: &str = "sha256-610f5ae4d76e332636a17bd357fd6ce99029316a99d320280d4d77a746bf29e8";
const CLIP_BYTES: &[u8] = b"RIFF\x04\x00\x00\x00WAVE";
const IMAGE_BYTES: &[u8] = b"\x89PNG\r\n\x1a\n";
const LONG_SHOW_MS: u64 = 60_000;
const SHORT_SHOW_MS: u64 = 1;
const POLL: Duration = Duration::from_millis(5);
const HANG_GUARD: Duration = Duration::from_secs(30);

fn blob_id(raw: &str) -> MediaBlobId {
    MediaBlobId::from_stored(raw).expect("fixture blob id is well formed")
}

fn blob(id: &str, format: MediaFormat, bytes: &[u8]) -> MediaBlob {
    MediaBlob {
        id: blob_id(id),
        format,
        byte_size: bytes.len() as u64,
        label: "fixture".to_owned(),
        imported_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn clip_row() -> StoredClip {
    StoredClip {
        id: CLIP_ULID
            .parse::<ClipId>()
            .expect("the fixture clip id is a ulid"),
        name: "Fanfare".to_owned(),
        file_path: PathBuf::from("/library/fanfare.wav"),
        volume: 1.0,
        output_device: OutputDevice::Default,
        hotkey: None,
        created_at: OffsetDateTime::UNIX_EPOCH,
        category: "General".to_owned(),
        loop_playback: false,
        duration_secs: None,
        builtin_id: None,
    }
}

fn media_library() -> OverlayMediaLibrary {
    let mut clips = MockSoundboardClipsRepo::new();
    clips
        .expect_get()
        .returning(|clip| Ok((clip.to_string() == CLIP_ULID).then(clip_row)));

    let mut blobs = MockMediaRepo::new();
    blobs.expect_blob_of().returning(|referrer| {
        Ok(match referrer.kind {
            MediaReferrerKind::SoundboardClip => Some(blob_id(CLIP_BLOB)),
            MediaReferrerKind::Overlay | MediaReferrerKind::Library => None,
        })
    });
    blobs.expect_get().returning(|id| {
        Ok(match id.as_str() {
            CLIP_BLOB => Some(blob(CLIP_BLOB, MediaFormat::Wav, CLIP_BYTES)),
            IMAGE_BLOB => Some(blob(IMAGE_BLOB, MediaFormat::Png, IMAGE_BYTES)),
            _ => None,
        })
    });
    blobs.expect_read().returning(|id| {
        Ok(match id.as_str() {
            CLIP_BLOB => CLIP_BYTES.to_vec(),
            _ => IMAGE_BYTES.to_vec(),
        })
    });
    blobs.expect_retain().returning(|_, _| Ok(()));
    blobs.expect_release().returning(|_| Ok(false));
    blobs.expect_release_all().returning(|_, _| Ok(0));

    OverlayMediaLibrary::new(
        Arc::new(blobs) as Arc<dyn MediaRepo>,
        Arc::new(clips) as Arc<dyn SoundboardClipsRepo>,
    )
}

#[derive(Default)]
struct RecordingSink {
    frames: Mutex<Vec<(OverlayId, Value)>>,
}

impl RecordingSink {
    fn frames_of(&self, id: &str) -> Vec<Value> {
        self.frames
            .lock()
            .unwrap()
            .iter()
            .filter(|(identity, _)| identity.as_str() == id)
            .map(|(_, content)| content.clone())
            .collect()
    }
}

#[async_trait]
impl OverlayFrameSink for RecordingSink {
    async fn deliver_content(
        &self,
        identity: &OverlayId,
        content: Value,
        _: Option<u64>,
    ) -> OverlayReceivers {
        self.frames
            .lock()
            .unwrap()
            .push((identity.clone(), content));
        OverlayReceivers {
            sources: 1,
            preview_tabs: 0,
        }
    }

    async fn deliver_reload(&self, _: &OverlayId) {}

    async fn revoke(&self, _: &OverlayId) {}
}

type Retained = Arc<Mutex<Vec<(OverlayId, OverlayConfig)>>>;

struct Harness {
    _home: TempDir,
    root: PathBuf,
    sink: Arc<RecordingSink>,
    retained: Retained,
    service: OverlayServiceHandle,
}

async fn harness(materialized: bool) -> Harness {
    let home = TempDir::new().expect("tempdir");
    let root = home.path().join("overlays");
    let without_exit_motion =
        OverlayConfig::from([(EXIT.to_owned(), Variant::String(NO_MOTION.to_owned()))]);
    let definitions = vec![
        common::overlay_definition(ALERT, ALERT, ALERT_KIND, without_exit_motion),
        common::overlay_definition(FRAME, FRAME, FRAME_KIND, OverlayConfig::new()),
    ];

    let retained: Retained = Arc::default();
    let mut repo = MockOverlayRepo::new();
    let listed = definitions.clone();
    repo.expect_list().returning(move || Ok(listed.clone()));
    repo.expect_get()
        .returning(move |id| Ok(definitions.iter().find(|d| &d.id == id).cloned()));
    repo.expect_save().returning(|_| Ok(()));
    repo.expect_delete().returning(|_| Ok(true));
    let kept = Arc::clone(&retained);
    repo.expect_set_retained_content()
        .returning(move |id, content| {
            kept.lock().unwrap().push((id.clone(), content.clone()));
            Ok(())
        });
    repo.expect_get_retained_content().returning(|_| Ok(None));

    let settings = MemorySettings::new();
    settings
        .seed(
            &core_settings::SERVER_OVERLAY_ROOT,
            &Some(root.to_string_lossy().into_owned()),
        )
        .expect("overlay root setting");

    let sink = Arc::new(RecordingSink::default());
    let service = OverlayServiceHandle::new(OverlayServiceParts {
        default_root: PathBuf::new(),
        date_names: forge_runtime::DateNamesHandle::default(),
        tasks: forge_net::TaskOwner::new(),
        repo: Arc::new(repo) as Arc<dyn OverlayRepo>,
        settings: Arc::new(settings) as Arc<dyn SettingsRepo>,
        kinds: common::builtin_overlay_kinds(),
        bus: EventBus::new(Arc::new(NullEventLogRepo)),
        frames: Arc::clone(&sink) as Arc<dyn OverlayFrameSink>,
        media: Some(media_library()),
        speaker: None,
        latest: None,
    });
    if materialized {
        for id in [ALERT, FRAME] {
            service
                .materialize(&OverlayId::new(id), &common::NeutralSamples)
                .await
                .expect("the overlay materializes");
        }
    }

    Harness {
        _home: home,
        root,
        sink,
        retained,
        service,
    }
}

fn step(entries: &[(&str, String)]) -> OverlayConfig {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), Variant::String(value.clone())))
        .collect()
}

fn picked_image_and_sound() -> OverlayConfig {
    step(&[
        (HEADLINE, "New follower".to_owned()),
        (SHOW_IMAGE, image_reference(IMAGE_BLOB)),
        (SHOW_SOUND, clip_reference(CLIP_ULID)),
    ])
}

fn show_media_path(file: &str) -> String {
    format!("{SHOW_MEDIA_DIRECTORY}/{file}")
}

impl Harness {
    fn overlay_dir(&self, id: &str) -> PathBuf {
        self.root.canonicalize().expect("the root exists").join(id)
    }

    fn show_media_dir(&self, id: &str) -> PathBuf {
        self.root.join(id).join(SHOW_MEDIA_DIRECTORY)
    }

    async fn send(&self, id: &str, content: &OverlayConfig, duration_ms: u64) -> OverlayDispatch {
        self.service
            .send_to(
                &OverlayId::new(id),
                content,
                &ArgStack::new(),
                OverlaySendOptions {
                    duration_ms: Some(duration_ms),
                    caused_by: None,
                },
            )
            .await
            .expect("a stored overlay of a shipped look accepts a send")
    }

    async fn first_frame(&self, id: &str) -> Value {
        wait_until(
            || !self.sink.frames_of(id).is_empty(),
            "the show never reached the page",
        )
        .await;
        self.sink.frames_of(id).remove(0)
    }
}

async fn wait_until(mut done: impl FnMut() -> bool, hung: &str) {
    tokio::time::timeout(HANG_GUARD, async {
        while !done() {
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{hung}"));
}

fn is_show_media_absent(dir: &Path) -> bool {
    !dir.exists()
}

#[tokio::test]
async fn a_picked_image_and_sound_reach_the_page_as_files_that_stay_while_the_show_is_up() {
    let harness = harness(true).await;

    harness
        .send(ALERT, &picked_image_and_sound(), LONG_SHOW_MS)
        .await;
    let frame = harness.first_frame(ALERT).await;

    let image = format!("{IMAGE_BLOB}.{}", MediaFormat::Png.as_str());
    let sound = format!("{CLIP_BLOB}.{}", MediaFormat::Wav.as_str());
    assert_eq!(frame[SHOW_IMAGE], show_media_path(&image));
    assert_eq!(frame[SHOW_SOUND], show_media_path(&sound));
    let dir = harness.overlay_dir(ALERT);
    assert_eq!(
        std::fs::read(dir.join(show_media_path(&image))).unwrap(),
        IMAGE_BYTES
    );
    assert_eq!(
        std::fs::read(dir.join(show_media_path(&sound))).unwrap(),
        CLIP_BYTES
    );
}

#[tokio::test]
async fn a_show_that_ends_takes_its_show_media_with_it() {
    let harness = harness(true).await;

    let OverlayDispatch::Queued(ticket) = harness
        .send(ALERT, &picked_image_and_sound(), SHORT_SHOW_MS)
        .await
    else {
        panic!("an alert show was not queued");
    };
    assert!(matches!(ticket.finished().await, ShowEnd::Shown(_)));

    let dir = harness.show_media_dir(ALERT);
    wait_until(
        || is_show_media_absent(&dir),
        "the show ended but its media stayed on disk",
    )
    .await;
}

#[tokio::test]
async fn deleting_an_overlay_clears_the_show_media_its_applied_show_still_held() {
    let harness = harness(true).await;
    let picked = step(&[(SHOW_SOUND, clip_reference(CLIP_ULID))]);

    let dispatch = harness.send(FRAME, &picked, LONG_SHOW_MS).await;
    assert!(matches!(dispatch, OverlayDispatch::Applied(_)));
    let dir = harness.show_media_dir(FRAME);
    assert!(dir.exists(), "the applied show released its media at once");

    harness
        .service
        .delete(&OverlayId::new(FRAME))
        .await
        .expect("the overlay deletes");

    wait_until(
        || is_show_media_absent(&dir),
        "the deleted overlay's show media stayed on disk",
    )
    .await;
}

#[tokio::test]
async fn kept_content_carries_no_show_media_while_the_delivered_frame_does() {
    let harness = harness(true).await;
    let picked = step(&[(SHOW_SOUND, clip_reference(CLIP_ULID))]);

    harness.send(FRAME, &picked, LONG_SHOW_MS).await;

    let frame = harness.first_frame(FRAME).await;
    assert!(frame[SHOW_SOUND].is_string(), "{frame}");
    let retained = harness.retained.lock().unwrap().clone();
    assert_eq!(retained.len(), 1);
    assert!(
        !retained[0].1.contains_key(SHOW_SOUND),
        "a reconnecting page would replay the show sound"
    );
}

#[tokio::test]
async fn a_show_without_a_usable_pick_plays_the_overlay_own_media_and_writes_nothing() {
    for (label, picked, materialized) in [
        (
            "blank step fields",
            step(&[(SHOW_SOUND, String::new())]),
            true,
        ),
        (
            "a clip the soundboard no longer holds",
            step(&[(SHOW_SOUND, clip_reference(UNKNOWN_CLIP_ULID))]),
            true,
        ),
        (
            "an overlay whose folder was never written",
            step(&[(SHOW_SOUND, clip_reference(CLIP_ULID))]),
            false,
        ),
    ] {
        let harness = harness(materialized).await;

        harness.send(FRAME, &picked, LONG_SHOW_MS).await;

        let frame = harness.first_frame(FRAME).await;
        assert!(
            frame.get(SHOW_SOUND).is_none(),
            "{label}: the page was handed {frame}"
        );
        assert!(
            is_show_media_absent(&harness.show_media_dir(FRAME)),
            "{label}: a show media folder was written"
        );
    }
}
