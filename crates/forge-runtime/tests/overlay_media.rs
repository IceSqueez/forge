#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use forge_overlay::config::{DEFAULT_ICON, HEADLINE, ICON, SOUND};
use forge_overlay::{
    CONFIG_FILE, GENERATED_MEDIA_DIRECTORY, ICON_FILE_FIELD, ICON_TINTABLE_FIELD,
    MaterializeReport, MediaIssue, OverlayKindRegistry, clip_reference, glyph_media,
    image_reference, register_builtin_kinds,
};
use forge_runtime::{EventBus, NullEventLogRepo, OverlayMediaLibrary, OverlayServiceHandle};
use forge_storage::settings::MockSettingsRepo;
use forge_storage::soundboard::MockSoundboardClipsRepo;
use forge_storage::{
    MediaBlob, MediaBlobId, MediaFormat, MediaReferrerKind, MediaRepo, MockMediaRepo,
    MockOverlayRepo, OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId, OverlayRepo,
    SettingsRepo, SoundboardClipsRepo, StorageError, StoredClip, reserved_keys,
};
use forge_types::{ClipId, OutputDevice, Variant};
use tempfile::TempDir;
use time::OffsetDateTime;

const ALERT_KIND: &str = "overlay.alert";
const OVERLAY: &str = "sub-alert";
const CLIP_ULID: &str = "01J9P4S2M7Q8V3X5Y6Z7A8B9C0";
const BLOB_ID: &str = "sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8";
const OTHER_BLOB_ID: &str =
    "sha256-610f5ae4d76e332636a17bd357fd6ce99029316a99d320280d4d77a746bf29e8";
const CLIP_BYTES: &[u8] = b"RIFF\x04\x00\x00\x00WAVE";
const STORE_OFFLINE: &str = "media store offline";

fn clip_id() -> ClipId {
    CLIP_ULID.parse().expect("the fixture clip id is a ulid")
}

fn blob(format: MediaFormat) -> MediaBlob {
    MediaBlob {
        id: MediaBlobId::from_stored(BLOB_ID),
        format,
        byte_size: CLIP_BYTES.len() as u64,
        label: "fanfare.wav".to_owned(),
        imported_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn clip_row() -> StoredClip {
    StoredClip {
        id: clip_id(),
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

#[derive(Default)]
struct MediaWorld {
    clip_row: Option<StoredClip>,
    clip_lookup_fails: bool,
    source_blob: Option<MediaBlobId>,
    blob: Option<MediaBlob>,
    bytes: Option<Vec<u8>>,
    slots: BTreeMap<String, MediaBlobId>,
    retained: Vec<(String, MediaBlobId)>,
    released: Vec<String>,
    released_all: Vec<(MediaReferrerKind, String)>,
}

impl MediaWorld {
    fn resolvable() -> Self {
        Self {
            clip_row: Some(clip_row()),
            source_blob: Some(MediaBlobId::from_stored(BLOB_ID)),
            blob: Some(blob(MediaFormat::Wav)),
            bytes: Some(CLIP_BYTES.to_vec()),
            ..Self::default()
        }
    }
}

type World = Arc<Mutex<MediaWorld>>;

fn offline() -> StorageError {
    StorageError::Connection {
        reason: STORE_OFFLINE.to_owned(),
    }
}

fn media_library(world: &World) -> OverlayMediaLibrary {
    let mut clips = MockSoundboardClipsRepo::new();
    let seen = Arc::clone(world);
    clips.expect_get().returning(move |_| {
        let seen = seen.lock().expect("world");
        if seen.clip_lookup_fails {
            return Err(offline());
        }
        Ok(seen.clip_row.clone())
    });

    let mut blobs = MockMediaRepo::new();

    let seen = Arc::clone(world);
    blobs.expect_blob_of().returning(move |referrer| {
        let seen = seen.lock().expect("world");
        Ok(match referrer.kind {
            MediaReferrerKind::SoundboardClip => seen.source_blob.clone(),
            MediaReferrerKind::Overlay => seen.slots.get(&referrer.slot).cloned(),
        })
    });

    let seen = Arc::clone(world);
    blobs
        .expect_get()
        .returning(move |_| Ok(seen.lock().expect("world").blob.clone()));

    let seen = Arc::clone(world);
    blobs.expect_read().returning(move |id| {
        seen.lock()
            .expect("world")
            .bytes
            .clone()
            .ok_or_else(|| StorageError::NotFound {
                key: id.as_str().to_owned(),
            })
    });

    let seen = Arc::clone(world);
    blobs.expect_retain().returning(move |referrer, id| {
        let mut seen = seen.lock().expect("world");
        seen.retained.push((referrer.slot.clone(), id.clone()));
        seen.slots.insert(referrer.slot.clone(), id.clone());
        Ok(())
    });

    let seen = Arc::clone(world);
    blobs.expect_release().returning(move |referrer| {
        let mut seen = seen.lock().expect("world");
        seen.released.push(referrer.slot.clone());
        Ok(seen.slots.remove(&referrer.slot).is_some())
    });

    let seen = Arc::clone(world);
    blobs.expect_release_all().returning(move |kind, id| {
        let mut seen = seen.lock().expect("world");
        seen.released_all.push((kind, id.to_owned()));
        let held = seen.slots.len() as u64;
        seen.slots.clear();
        Ok(held)
    });

    OverlayMediaLibrary::new(
        Arc::new(blobs) as Arc<dyn MediaRepo>,
        Arc::new(clips) as Arc<dyn SoundboardClipsRepo>,
    )
}

struct Harness {
    _home: TempDir,
    root: PathBuf,
    world: World,
    service: OverlayServiceHandle,
}

impl Harness {
    fn document(&self) -> serde_json::Value {
        let path = self
            .root
            .canonicalize()
            .expect("the root exists")
            .join(OVERLAY)
            .join(CONFIG_FILE);
        serde_json::from_str(&std::fs::read_to_string(path).expect("the document is on disk"))
            .expect("the document is json")
    }

    fn sound(&self) -> String {
        self.document()["config"][SOUND]
            .as_str()
            .expect("the page always carries a sound value")
            .to_owned()
    }

    fn icon(&self) -> serde_json::Value {
        self.document()["config"][ICON].clone()
    }

    fn icon_file(&self) -> String {
        self.icon()[ICON_FILE_FIELD]
            .as_str()
            .expect("the page always carries an icon file name")
            .to_owned()
    }

    fn world(&self) -> std::sync::MutexGuard<'_, MediaWorld> {
        self.world.lock().expect("world")
    }
}

fn default_glyph_file() -> String {
    let stored: OverlayConfig = [(ICON.to_owned(), Variant::String(DEFAULT_ICON.to_owned()))]
        .into_iter()
        .collect();

    glyph_media(&stored)
        .resolved
        .first()
        .expect("the icon an untouched alert draws resolves without a library")
        .file_name()
        .to_owned()
}

fn definition(config: OverlayConfig) -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new(OVERLAY),
        display_name: "Sub alert".to_owned(),
        kind_id: ALERT_KIND.to_owned(),
        enabled: true,
        position: 0,
        config,
        config_schema_version: 1,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new("2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d"),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn sound_config(value: &str) -> OverlayConfig {
    [(SOUND.to_owned(), Variant::String(value.to_owned()))]
        .into_iter()
        .collect()
}

fn icon_config(value: &str) -> OverlayConfig {
    [
        (SOUND.to_owned(), Variant::String(String::new())),
        (ICON.to_owned(), Variant::String(value.to_owned())),
    ]
    .into_iter()
    .collect()
}

fn registry() -> Arc<OverlayKindRegistry> {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    Arc::new(reg)
}

fn harness(config: OverlayConfig, library_world: Option<MediaWorld>) -> Harness {
    let home = TempDir::new().expect("tempdir");
    let root = home.path().join("overlays");
    let stored = definition(config);

    let mut repo = MockOverlayRepo::new();
    let listed = vec![stored.clone()];
    repo.expect_list().returning(move || Ok(listed.clone()));
    let one = stored;
    repo.expect_get()
        .returning(move |id| Ok((&one.id == id).then(|| one.clone())));
    repo.expect_save().returning(|_| Ok(()));

    let mut settings = MockSettingsRepo::new();
    let configured = root.to_string_lossy().into_owned();
    settings
        .expect_get_string()
        .returning(move |key| match key {
            reserved_keys::SERVER_OVERLAY_ROOT => Ok(Some(configured.clone())),
            _ => Ok(None),
        });

    let wire_library = library_world.is_some();
    let world: World = Arc::new(Mutex::new(library_world.unwrap_or_default()));

    let mut service = OverlayServiceHandle::new(
        Arc::new(repo) as Arc<dyn OverlayRepo>,
        Arc::new(settings) as Arc<dyn SettingsRepo>,
        registry(),
        EventBus::new(Arc::new(NullEventLogRepo)),
        None,
    );
    if wire_library {
        service = service.with_media_library(media_library(&world));
    }

    Harness {
        _home: home,
        root,
        world,
        service,
    }
}

async fn pass(harness: &Harness) -> MaterializeReport {
    harness
        .service
        .materialize(&OverlayId::new(OVERLAY))
        .await
        .expect("the pass completes")
}

#[tokio::test]
async fn every_way_a_reference_can_fail_leaves_the_page_silent_and_names_the_reason() {
    for (world, expected, label) in [
        (
            MediaWorld::default(),
            MediaIssue::UnknownClip {
                key: SOUND.to_owned(),
                clip: CLIP_ULID.to_owned(),
            },
            "a clip the soundboard no longer holds",
        ),
        (
            MediaWorld {
                clip_row: Some(clip_row()),
                ..MediaWorld::default()
            },
            MediaIssue::ClipOutsideLibrary {
                key: SOUND.to_owned(),
                clip: CLIP_ULID.to_owned(),
            },
            "a clip that was never copied into the library",
        ),
        (
            MediaWorld {
                blob: None,
                ..MediaWorld::resolvable()
            },
            MediaIssue::ClipBytesMissing {
                key: SOUND.to_owned(),
                clip: CLIP_ULID.to_owned(),
            },
            "a reference whose blob row is gone",
        ),
        (
            MediaWorld {
                bytes: None,
                ..MediaWorld::resolvable()
            },
            MediaIssue::ClipBytesMissing {
                key: SOUND.to_owned(),
                clip: CLIP_ULID.to_owned(),
            },
            "a blob row whose file is gone",
        ),
        (
            MediaWorld {
                blob: Some(blob(MediaFormat::Png)),
                ..MediaWorld::resolvable()
            },
            MediaIssue::ClipKindMismatch {
                key: SOUND.to_owned(),
                clip: CLIP_ULID.to_owned(),
                format: MediaFormat::Png.to_string(),
            },
            "a picture stored under the sound key",
        ),
        (
            MediaWorld {
                clip_lookup_fails: true,
                ..MediaWorld::resolvable()
            },
            MediaIssue::LookupFailed {
                key: SOUND.to_owned(),
                reference: CLIP_ULID.to_owned(),
                reason: offline().to_string(),
            },
            "a store that cannot answer",
        ),
    ] {
        let harness = harness(sound_config(&format!("clip:{CLIP_ULID}")), Some(world));

        let report = pass(&harness).await;

        assert_eq!(report.media_issues, vec![expected], "{label}");
        assert_eq!(
            harness.sound(),
            "",
            "{label} must leave the page with no sound filename"
        );
        assert_eq!(
            report.media_written,
            vec![default_glyph_file()],
            "{label} wrote a file for the reference it could not resolve, or dropped the rest of \
             the pass"
        );
    }
}

#[tokio::test]
async fn a_clip_id_the_library_could_never_hold_is_reported_without_a_lookup() {
    let harness = harness(sound_config("clip:not-a-ulid"), Some(MediaWorld::default()));

    let report = pass(&harness).await;

    assert_eq!(
        report.media_issues,
        vec![MediaIssue::UnknownClip {
            key: SOUND.to_owned(),
            clip: "not-a-ulid".to_owned(),
        }]
    );
}

#[tokio::test]
async fn a_resolved_reference_is_written_to_the_namespace_and_named_on_the_page() {
    let harness = harness(
        sound_config(&format!("clip:{CLIP_ULID}")),
        Some(MediaWorld::resolvable()),
    );

    let report = pass(&harness).await;

    let file = format!("{BLOB_ID}.{}", MediaFormat::Wav.as_str());
    assert!(report.media_issues.is_empty());
    assert_eq!(
        report.media_written,
        vec![default_glyph_file(), file.clone()]
    );
    assert_eq!(
        harness.sound(),
        format!("{GENERATED_MEDIA_DIRECTORY}/{file}")
    );
    assert_eq!(
        std::fs::read(report.directory.join(GENERATED_MEDIA_DIRECTORY).join(&file))
            .expect("the generated file is on disk"),
        CLIP_BYTES
    );
}

#[tokio::test]
async fn a_curated_glyph_reaches_the_page_even_when_the_service_has_no_media_library() {
    let harness = harness(icon_config(DEFAULT_ICON), None);

    let report = pass(&harness).await;

    let glyph = default_glyph_file();
    assert!(report.media_issues.is_empty(), "{:?}", report.media_issues);
    assert_eq!(report.media_written, vec![glyph.clone()]);
    assert_eq!(
        harness.icon()[ICON_FILE_FIELD].as_str(),
        Some(format!("{GENERATED_MEDIA_DIRECTORY}/{glyph}").as_str()),
        "a glyph needs no library and still did not reach the page"
    );
    assert_eq!(
        harness.icon()[ICON_TINTABLE_FIELD].as_bool(),
        Some(true),
        "a curated glyph reached the page as something the accent may not fill"
    );
}

#[tokio::test]
async fn an_icon_naming_a_glyph_this_build_does_not_carry_leaves_the_page_iconless() {
    for stored in ["no-such-glyph", &clip_reference(CLIP_ULID)] {
        let harness = harness(icon_config(stored), Some(MediaWorld::resolvable()));

        let report = pass(&harness).await;

        assert_eq!(
            report.media_issues,
            vec![MediaIssue::UnknownGlyph {
                key: ICON.to_owned(),
                glyph: stored.to_owned(),
            }],
            "{stored:?} was answered with something other than an unknown glyph"
        );
        assert_eq!(harness.icon_file(), "", "{stored:?} still named a file");
        assert!(
            report.media_written.is_empty(),
            "{stored:?} wrote a generated file"
        );
    }
}

#[tokio::test]
async fn an_icon_naming_a_blob_that_is_not_a_picture_leaves_the_page_iconless() {
    let harness = harness(
        icon_config(&image_reference(BLOB_ID)),
        Some(MediaWorld::resolvable()),
    );

    let report = pass(&harness).await;

    assert_eq!(
        report.media_issues,
        vec![MediaIssue::ImageKindMismatch {
            key: ICON.to_owned(),
            image: BLOB_ID.to_owned(),
            format: MediaFormat::Wav.to_string(),
        }]
    );
    assert_eq!(harness.icon_file(), "");
}

#[tokio::test]
async fn an_icon_reference_shaped_like_a_path_is_refused_before_anything_is_written() {
    let escaping = "../../etc/passwd";
    let harness = harness(
        icon_config(&image_reference(escaping)),
        Some(MediaWorld {
            blob: Some(blob(MediaFormat::Png)),
            ..MediaWorld::resolvable()
        }),
    );

    let report = pass(&harness).await;

    assert!(
        matches!(
            report.media_issues.as_slice(),
            [MediaIssue::LookupFailed { key, reference, .. }]
                if key == ICON && reference == escaping
        ),
        "{:?}",
        report.media_issues
    );
    assert_eq!(harness.icon_file(), "");
    assert!(
        report.media_written.is_empty(),
        "{:?} was written",
        report.media_written
    );
}

#[tokio::test]
async fn an_imported_icon_is_drawn_as_imported_and_holds_the_slot_a_glyph_gives_back() {
    let imported = harness(
        icon_config(&image_reference(BLOB_ID)),
        Some(MediaWorld {
            blob: Some(blob(MediaFormat::Png)),
            ..MediaWorld::resolvable()
        }),
    );

    let report = pass(&imported).await;

    let file = format!("{BLOB_ID}.{}", MediaFormat::Png.as_str());
    assert!(report.media_issues.is_empty(), "{:?}", report.media_issues);
    assert_eq!(
        imported.icon()[ICON_FILE_FIELD].as_str(),
        Some(format!("{GENERATED_MEDIA_DIRECTORY}/{file}").as_str())
    );
    assert_eq!(
        imported.icon()[ICON_TINTABLE_FIELD].as_bool(),
        Some(false),
        "an imported image reached the page as something the accent may repaint"
    );
    assert_eq!(
        imported.world().retained,
        vec![(ICON.to_owned(), MediaBlobId::from_stored(BLOB_ID))],
        "an imported icon did not hold the blob it draws"
    );

    let mut world = MediaWorld::resolvable();
    world
        .slots
        .insert(ICON.to_owned(), MediaBlobId::from_stored(BLOB_ID));
    let switched = harness(icon_config(DEFAULT_ICON), Some(world));

    pass(&switched).await;

    assert_eq!(
        switched.world().released,
        vec![ICON.to_owned()],
        "switching to a curated glyph left the imported image held forever"
    );
}

#[tokio::test]
async fn a_resolved_reference_holds_the_blob_for_the_slot_it_fills() {
    let harness = harness(
        sound_config(&format!("clip:{CLIP_ULID}")),
        Some(MediaWorld::resolvable()),
    );

    pass(&harness).await;

    let world = harness.world();
    assert_eq!(
        world.retained,
        vec![(SOUND.to_owned(), MediaBlobId::from_stored(BLOB_ID))]
    );
    assert!(world.released.is_empty());
}

#[tokio::test]
async fn a_slot_already_holding_the_same_blob_is_left_alone_instead_of_retained_again() {
    let mut world = MediaWorld::resolvable();
    world
        .slots
        .insert(SOUND.to_owned(), MediaBlobId::from_stored(BLOB_ID));
    let harness = harness(sound_config(&format!("clip:{CLIP_ULID}")), Some(world));

    pass(&harness).await;

    let world = harness.world();
    assert!(world.retained.is_empty(), "an unchanged slot was rewritten");
    assert!(world.released.is_empty());
}

#[tokio::test]
async fn a_slot_whose_key_stops_naming_a_clip_is_released() {
    let mut world = MediaWorld::resolvable();
    world
        .slots
        .insert(SOUND.to_owned(), MediaBlobId::from_stored(OTHER_BLOB_ID));
    let harness = harness(sound_config("fanfare.mp3"), Some(world));

    pass(&harness).await;

    let world = harness.world();
    assert_eq!(world.released, vec![SOUND.to_owned()]);
    assert!(world.retained.is_empty());
}

#[tokio::test]
async fn a_transient_lookup_failure_leaves_the_slot_exactly_as_it_was() {
    let mut world = MediaWorld::resolvable();
    world.clip_lookup_fails = true;
    world
        .slots
        .insert(SOUND.to_owned(), MediaBlobId::from_stored(BLOB_ID));
    let harness = harness(sound_config(&format!("clip:{CLIP_ULID}")), Some(world));

    pass(&harness).await;

    let world = harness.world();
    assert!(
        world.released.is_empty() && world.retained.is_empty(),
        "an unreadable store dropped a reference the record still names"
    );
}

#[tokio::test]
async fn a_service_with_no_media_library_reports_every_reference_instead_of_dropping_it() {
    let harness = harness(sound_config(&format!("clip:{CLIP_ULID}")), None);

    let report = pass(&harness).await;

    assert!(
        matches!(
            report.media_issues.as_slice(),
            [MediaIssue::LookupFailed { key, reference, .. }]
                if key == SOUND && reference == CLIP_ULID
        ),
        "{:?}",
        report.media_issues
    );
    assert_eq!(harness.sound(), "");
}

#[tokio::test]
async fn a_key_that_does_not_hold_media_keeps_wording_that_opens_with_the_prefix() {
    let mut config = sound_config("");
    config.insert(
        HEADLINE.to_owned(),
        Variant::String(format!("clip:{CLIP_ULID}")),
    );
    let harness = harness(config, Some(MediaWorld::resolvable()));

    let report = pass(&harness).await;

    assert!(report.media_issues.is_empty());
    assert_eq!(
        harness.document()["config"][HEADLINE]
            .as_str()
            .expect("the headline is a string"),
        format!("clip:{CLIP_ULID}"),
        "a headline that opens with the reference prefix was rewritten as media"
    );
}

#[tokio::test]
async fn deleting_an_overlay_drops_every_slot_it_held() {
    let mut world = MediaWorld::resolvable();
    world
        .slots
        .insert(SOUND.to_owned(), MediaBlobId::from_stored(BLOB_ID));
    let harness = harness(sound_config(&format!("clip:{CLIP_ULID}")), Some(world));

    let dropped = harness
        .service
        .release_media(&OverlayId::new(OVERLAY))
        .await
        .expect("releasing a record's media is not an error");

    assert_eq!(dropped, 1);
    assert_eq!(
        harness.world().released_all,
        vec![(MediaReferrerKind::Overlay, OVERLAY.to_owned())]
    );
}

#[tokio::test]
async fn releasing_media_without_a_library_is_a_quiet_no_op() {
    let harness = harness(sound_config(""), None);

    assert_eq!(
        harness
            .service
            .release_media(&OverlayId::new(OVERLAY))
            .await
            .expect("an unwired service must not fail the delete flow"),
        0
    );
}
