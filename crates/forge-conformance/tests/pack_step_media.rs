#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_overlay::{SHOW_IMAGE, SHOW_SOUND};
use forge_registry::InternalNamespace;
use forge_storage::{MediaBlobId, OverlayId, StoredClip, clip_source_referrer, library_referrer};
use forge_transfer::{
    PackConflict, PackExportRequest, PackImportOutcome, PackItemKey, PackItemKind, PackMetadata,
    PackReference, PackResolution, PackSelection, TransferError, apply_pack_import,
    load_pack_inventory, plan_pack_import, resolve_pack_closure, stage_pack, write_pack,
};
use forge_types::{Action, ActionId, ClipId, OutputDevice, Queue, QueueId};
use time::OffsetDateTime;
use ulid::Ulid;

use forge_host::test_support::pack_fixture::{
    OGG, OVERLAY_KIND, OVERLAY_NAME, PNG, Registries, Store, action, config, step, text,
};

const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00wave";
const STRAY_OGG: &[u8] = b"OggS\x00\x02\x00\x00stray";
const RETIRED_KIND: &str = "overlay.retired_kind";

struct StepMedia {
    root: ActionId,
    overlay: OverlayId,
    clip: ClipId,
    gif: MediaBlobId,
    ding: MediaBlobId,
}

async fn seed_send_step(store: &Store, show_image: &str) -> StepMedia {
    let p = store.provider.as_ref();
    let queue = Queue {
        id: QueueId::new(),
        name: "Step media queue".to_owned(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::new(1).unwrap(),
    };
    p.queue_repo().save(&queue).await.unwrap();
    let media = p.media_repo();
    let gif = media.store("wave.gif", GIF.to_vec()).await.unwrap().id;
    let ding = media.store("ding.ogg", OGG.to_vec()).await.unwrap().id;
    let clip = StoredClip {
        id: ClipId::new(),
        name: "Ding".to_owned(),
        file_path: media.resolve(&ding).await.unwrap(),
        volume: 1.0,
        output_device: OutputDevice::Default,
        hotkey: None,
        created_at: OffsetDateTime::now_utc(),
        category: String::new(),
        loop_playback: false,
        duration_secs: None,
        builtin_id: None,
    };
    p.soundboard_clips_repo().save(&clip).await.unwrap();
    media
        .retain(&clip_source_referrer(clip.id), &ding)
        .await
        .unwrap();
    let overlay = p
        .overlay_repo()
        .create(OVERLAY_NAME, OVERLAY_KIND, 1)
        .await
        .unwrap();
    let show_image = show_image.replace("{gif}", gif.as_str());
    let root = action(
        ActionId::new(),
        "Show wave",
        queue.id,
        vec![step(
            forge_runtime::OVERLAY_SEND_KIND_ID,
            config(&[
                (forge_runtime::OVERLAY_TARGET_KEY, text(overlay.id.as_str())),
                (SHOW_IMAGE, text(&show_image)),
                (SHOW_SOUND, text(&format!("clip:{}", clip.id))),
            ]),
        )],
    );
    p.action_repo().save(&root).await.unwrap();
    StepMedia {
        root: root.id,
        overlay: overlay.id,
        clip: clip.id,
        gif,
        ding,
    }
}

async fn seed(store: &Store) -> StepMedia {
    seed_send_step(store, "image:{gif}").await
}

async fn select(store: &Store, registries: &Registries, root: ActionId) -> PackSelection {
    let inventory = load_pack_inventory(store.provider.as_ref()).await.unwrap();
    resolve_pack_closure(&inventory, registries.closure(), &[root]).unwrap()
}

async fn export_with(
    store: &Store,
    registries: &Registries,
    root: ActionId,
    extra_media: &[&MediaBlobId],
) -> PathBuf {
    let inventory = load_pack_inventory(store.provider.as_ref()).await.unwrap();
    let mut selection = resolve_pack_closure(&inventory, registries.closure(), &[root]).unwrap();
    selection
        .media
        .extend(extra_media.iter().map(|id| (*id).clone()));
    write_pack(PackExportRequest {
        provider: Arc::clone(&store.provider),
        inventory,
        selection,
        metadata: PackMetadata {
            name: "Step media".to_owned(),
            author: "QA".to_owned(),
            description: String::new(),
        },
        overlay_root: store.overlay_root(),
        destination: store
            .data_dir()
            .join(format!("{}.forgebundle", Ulid::generate())),
    })
    .await
    .unwrap()
}

async fn import(
    store: &Store,
    registries: &Registries,
    bundle: &Path,
    resolution: Option<PackResolution>,
) -> Result<PackImportOutcome, TransferError> {
    let staged = stage_pack(bundle.to_path_buf(), store.data_dir().to_path_buf())
        .await
        .unwrap();
    let inventory = load_pack_inventory(store.provider.as_ref()).await.unwrap();
    let mut plan = plan_pack_import(&staged, &inventory, registries.closure()).unwrap();
    if let Some(resolution) = resolution {
        let conflicted: Vec<_> = plan
            .items()
            .iter()
            .filter(|item| item.conflict != PackConflict::Clear)
            .map(|item| item.key.clone())
            .collect();
        for key in conflicted {
            plan.set_resolution(&key, resolution).unwrap();
        }
    }
    apply_pack_import(forge_transfer::PackImportRequest {
        provider: Arc::clone(&store.provider),
        registries: registries.closure(),
        staged: &staged,
        plan,
        data_dir: store.data_dir().to_path_buf(),
        overlay_root: store.overlay_root(),
    })
    .await
}

fn local_id(outcome: &PackImportOutcome, kind: PackItemKind, pack_id: &str) -> String {
    outcome
        .written
        .iter()
        .find(|item| item.key.kind == kind && item.key.id == pack_id)
        .unwrap()
        .local_id
        .clone()
}

fn send_value(action: &Action, key: &str) -> String {
    action
        .sub_actions
        .iter()
        .find(|step| step.kind_id == forge_runtime::OVERLAY_SEND_KIND_ID)
        .and_then(|step| step.config.get(key))
        .and_then(|value| value.as_str())
        .unwrap()
        .to_owned()
}

async fn library_held(store: &Store, blob: &MediaBlobId) -> bool {
    store
        .provider
        .media_repo()
        .blob_of(&library_referrer(blob))
        .await
        .unwrap()
        .as_ref()
        == Some(blob)
}

async fn copied_send_values(
    store: &Store,
    outcome: &PackImportOutcome,
    f: &StepMedia,
) -> Vec<String> {
    let copy = local_id(outcome, PackItemKind::Action, &f.root.to_string());
    let copied = store.action(copy.parse().unwrap()).await.unwrap();
    [forge_runtime::OVERLAY_TARGET_KEY, SHOW_SOUND, SHOW_IMAGE]
        .map(|key| send_value(&copied, key))
        .to_vec()
}

fn copied_targets(outcome: &PackImportOutcome, f: &StepMedia) -> Vec<String> {
    let clip = local_id(outcome, PackItemKind::Clip, &f.clip.to_string());
    let image = local_id(outcome, PackItemKind::Media, f.gif.as_str());
    vec![
        local_id(outcome, PackItemKind::Overlay, f.overlay.as_str()),
        format!("clip:{clip}"),
        format!("image:{image}"),
    ]
}

#[tokio::test]
async fn the_closure_carries_the_clip_and_image_picked_only_in_a_send_steps_show_fields() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;

    let selection = select(&store, &registries, f.root).await;

    assert_eq!(
        (selection.clips, selection.media),
        (BTreeSet::from([f.clip]), BTreeSet::from([f.gif, f.ding]),)
    );
}

#[tokio::test]
async fn a_show_image_naming_a_blob_the_store_lacks_is_reported_unresolved() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let absent = MediaBlobId::from_digest(&[0xab; 32]);
    let f = seed_send_step(&store, &format!("image:{}", absent.as_str())).await;

    let selection = select(&store, &registries, f.root).await;

    assert_eq!(
        selection.unresolved_references,
        BTreeSet::from([PackReference {
            namespace: InternalNamespace::ManagedMedia.as_str().to_owned(),
            value: absent.as_str().to_owned(),
        }])
    );
}

#[tokio::test]
async fn importing_writes_the_step_image_and_a_library_only_image_under_the_library_hold() {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let f = seed(&source).await;
    let library_only = source
        .provider
        .media_repo()
        .store("logo.png", PNG.to_vec())
        .await
        .unwrap()
        .id;
    let bundle = export_with(&source, &registries, f.root, &[&library_only]).await;
    let target = Store::open().await;

    import(&target, &registries, &bundle, None).await.unwrap();

    assert_eq!(
        (
            library_held(&target, &f.gif).await,
            library_held(&target, &library_only).await,
        ),
        (true, true)
    );
}

#[tokio::test]
async fn an_audio_blob_no_written_clip_holds_is_not_written_and_is_reported_skipped() {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let f = seed(&source).await;
    let stray = source
        .provider
        .media_repo()
        .store("stray.ogg", STRAY_OGG.to_vec())
        .await
        .unwrap()
        .id;
    let bundle = export_with(&source, &registries, f.root, &[&stray]).await;
    let target = Store::open().await;

    let outcome = import(&target, &registries, &bundle, None).await.unwrap();

    assert_eq!(
        (
            target.provider.media_repo().get(&stray).await.unwrap(),
            outcome.skipped.contains(&PackItemKey {
                kind: PackItemKind::Media,
                id: stray.as_str().to_owned(),
            }),
        ),
        (None, true)
    );
}

#[tokio::test]
async fn keep_both_points_the_copied_send_step_at_the_copied_overlay_clip_and_image() {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let f = seed(&source).await;
    let bundle = export_with(&source, &registries, f.root, &[]).await;
    let target = Store::open().await;
    import(&target, &registries, &bundle, None).await.unwrap();

    let outcome = import(
        &target,
        &registries,
        &bundle,
        Some(PackResolution::KeepBoth),
    )
    .await
    .unwrap();

    assert_ne!(
        local_id(&outcome, PackItemKind::Clip, &f.clip.to_string()),
        f.clip.to_string(),
        "keep both must copy the clip under a new id"
    );
    assert_eq!(
        copied_send_values(&target, &outcome, &f).await,
        copied_targets(&outcome, &f)
    );
}

#[tokio::test]
async fn the_packs_overlay_kind_decides_the_show_fields_over_a_local_overlay_with_the_same_id() {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let f = seed(&source).await;
    let bundle = export_with(&source, &registries, f.root, &[]).await;
    let target = Store::open().await;
    import(&target, &registries, &bundle, None).await.unwrap();
    let overlays = target.provider.overlay_repo();
    let mut local = overlays.get(&f.overlay).await.unwrap().unwrap();
    local.kind_id = RETIRED_KIND.to_owned();
    overlays.save(&local).await.unwrap();

    let outcome = import(
        &target,
        &registries,
        &bundle,
        Some(PackResolution::KeepBoth),
    )
    .await
    .unwrap();

    let copy = local_id(&outcome, PackItemKind::Action, &f.root.to_string());
    let copied = target.action(copy.parse().unwrap()).await.unwrap();
    assert_eq!(
        send_value(&copied, SHOW_SOUND),
        format!(
            "clip:{}",
            local_id(&outcome, PackItemKind::Clip, &f.clip.to_string())
        )
    );
}
