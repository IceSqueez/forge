#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_registry::ReferenceNamespace;
use forge_storage::{GlobalsRepo, ScriptRecord, ScriptRepo};
use forge_storage_sqlite::SqliteDatabaseFileSchema;
use forge_transfer::{
    ExecutableContent, PackConflict, PackExportRequest, PackImportOutcome, PackImportPlan,
    PackImportRequest, PackItemKey, PackItemKind, PackMetadata, PackReference, PackResolution,
    PackSelection, StageRequest, StagedPack, TransferError, apply_pack_import, load_pack_inventory,
    plan_pack_import, resolve_pack_closure, stage_full_restore, stage_pack, write_pack,
};
use forge_types::{Action, ActionId, Queue, QueueId, ScriptId, SubActionStep, Variant};
use ulid::Ulid;

use forge_host::test_support::pack_fixture::*;

async fn select(store: &Store, registries: &Registries, roots: &[ActionId]) -> PackSelection {
    let inventory = load_pack_inventory(store.provider.as_ref()).await.unwrap();
    resolve_pack_closure(&inventory, registries.closure(), roots).unwrap()
}

async fn export_with(
    store: &Store,
    registries: &Registries,
    roots: &[ActionId],
    adjust: impl FnOnce(&mut PackSelection),
) -> PathBuf {
    let inventory = load_pack_inventory(store.provider.as_ref()).await.unwrap();
    let mut selection = resolve_pack_closure(&inventory, registries.closure(), roots).unwrap();
    adjust(&mut selection);
    let destination = store
        .data_dir()
        .join(format!("{}.forgebundle", Ulid::generate()));
    write_pack(PackExportRequest {
        provider: Arc::clone(&store.provider),
        inventory,
        selection,
        metadata: PackMetadata {
            name: "Showcase".to_owned(),
            author: "QA".to_owned(),
            description: "every dependency kind".to_owned(),
        },
        overlay_root: store.overlay_root(),
        destination,
    })
    .await
    .unwrap()
}

async fn export(store: &Store, registries: &Registries, roots: &[ActionId]) -> PathBuf {
    export_with(store, registries, roots, |_| {}).await
}

async fn preview(
    store: &Store,
    registries: &Registries,
    bundle: &Path,
) -> (StagedPack, PackImportPlan) {
    let staged = stage_pack(bundle.to_path_buf(), store.data_dir().to_path_buf())
        .await
        .unwrap();
    let inventory = load_pack_inventory(store.provider.as_ref()).await.unwrap();
    let plan = plan_pack_import(&staged, &inventory, registries.closure()).unwrap();
    (staged, plan)
}

async fn apply(
    store: &Store,
    registries: &Registries,
    staged: &StagedPack,
    plan: PackImportPlan,
) -> Result<PackImportOutcome, TransferError> {
    apply_pack_import(PackImportRequest {
        provider: Arc::clone(&store.provider),
        registries: registries.closure(),
        staged,
        plan,
        data_dir: store.data_dir().to_path_buf(),
        overlay_root: store.overlay_root(),
    })
    .await
}

async fn import(
    store: &Store,
    registries: &Registries,
    bundle: &Path,
    resolve: impl Fn(&PackConflict) -> Option<PackResolution>,
) -> Result<PackImportOutcome, TransferError> {
    let (staged, mut plan) = preview(store, registries, bundle).await;
    let chosen: Vec<_> = plan
        .items()
        .iter()
        .filter_map(|item| resolve(&item.conflict).map(|r| (item.key.clone(), r)))
        .collect();
    for (key, resolution) in chosen {
        plan.set_resolution(&key, resolution).unwrap();
    }
    apply(store, registries, &staged, plan).await
}

fn defaults(_: &PackConflict) -> Option<PackResolution> {
    None
}

fn on_conflict(resolution: PackResolution) -> impl Fn(&PackConflict) -> Option<PackResolution> {
    move |conflict| (*conflict != PackConflict::Clear).then_some(resolution)
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

fn step_value(action: &Action, kind_id: &str, key: &str) -> String {
    action
        .sub_actions
        .iter()
        .find(|step| step.kind_id == kind_id)
        .and_then(|step| step.config.get(key))
        .and_then(Variant::as_str)
        .unwrap()
        .to_owned()
}

async fn round_trip() -> (Registries, Store, Fixture, Store) {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let fixture = seed(&source).await;
    let bundle = export(&source, &registries, &[fixture.root]).await;
    let target = Store::open().await;
    import(&target, &registries, &bundle, defaults)
        .await
        .unwrap();
    (registries, source, fixture, target)
}

#[tokio::test]
async fn closure_pulls_every_transitive_internal_dependency_of_the_chosen_action() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;

    let selection = select(&store, &registries, &[f.root]).await;

    assert_eq!(
        (
            selection.actions,
            selection.queues,
            selection.scripts,
            selection.globals,
            selection.clips,
            selection.overlays,
            selection.media,
        ),
        (
            BTreeSet::from([f.root, f.helper]),
            BTreeSet::from([f.queue]),
            BTreeSet::from([SCRIPT_NAME.to_owned()]),
            BTreeSet::from([GLOBAL_NAME.to_owned()]),
            BTreeSet::from([f.clip]),
            BTreeSet::from([f.overlay.clone()]),
            BTreeSet::from([f.ding.clone(), f.icon.clone()]),
        )
    );
}

#[tokio::test]
async fn closure_takes_the_roots_linked_triggers_but_not_those_of_an_action_reached_by_reference() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;

    let selection = select(&store, &registries, &[f.root]).await;

    assert_eq!(
        selection.triggers,
        BTreeSet::from([f.root_trigger, f.toggled_trigger]),
        "the helper's own trigger {} must stay behind",
        f.helper_trigger
    );
}

#[tokio::test]
async fn an_integration_owned_reference_lands_in_the_manifest_requirements_not_in_the_pack() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;
    let obs = registries
        .sub_actions
        .owning_integration(SCENE_STEP)
        .unwrap()
        .clone();

    let bundle = export(&store, &registries, &[f.root]).await;
    let staged = stage_pack(bundle, store.data_dir().to_path_buf())
        .await
        .unwrap();

    let manifest = staged.manifest();
    assert!(manifest.required_integrations.contains(&obs));
    assert!(manifest.external_references.contains(&PackReference {
        namespace: "obs.scene_names".to_owned(),
        value: SCENE.to_owned(),
    }));
}

#[tokio::test]
async fn a_pack_carries_no_credentials_settings_or_unselected_actions() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;

    let bundle = export(&store, &registries, &[f.root]).await;
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(std::fs::File::open(&bundle).unwrap())
        .read_to_end(&mut raw)
        .unwrap();
    let raw = String::from_utf8_lossy(&raw);

    for leaked in [
        SECRET_TOKEN,
        SECRET_SETTING,
        "twitch:broadcaster",
        &f.unrelated.to_string(),
        "Unrelated trigger",
    ] {
        assert!(!raw.contains(leaked), "pack bytes carry {leaked:?}");
    }
}

#[tokio::test]
async fn exporting_without_a_known_root_action_is_refused_as_an_empty_pack() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    seed(&store).await;
    let inventory = load_pack_inventory(store.provider.as_ref()).await.unwrap();

    for roots in [Vec::new(), vec![ActionId::new()]] {
        let refused = resolve_pack_closure(&inventory, registries.closure(), &roots);
        assert!(
            matches!(refused, Err(TransferError::EmptyPack)),
            "{roots:?}: {refused:?}"
        );
    }
}

#[tokio::test]
async fn exporting_with_a_blank_pack_name_is_refused() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;
    let inventory = load_pack_inventory(store.provider.as_ref()).await.unwrap();
    let selection = resolve_pack_closure(&inventory, registries.closure(), &[f.root]).unwrap();
    let destination = store.data_dir().join("blank.forgebundle");

    let refused = write_pack(PackExportRequest {
        provider: Arc::clone(&store.provider),
        inventory,
        selection,
        metadata: PackMetadata {
            name: " \t ".to_owned(),
            author: String::new(),
            description: String::new(),
        },
        overlay_root: store.overlay_root(),
        destination: destination.clone(),
    })
    .await;

    assert!(matches!(refused, Err(TransferError::PackNameRequired)));
    assert!(!destination.exists());
}

#[tokio::test]
async fn importing_into_an_empty_store_recreates_the_actions_queue_and_script_under_their_ids() {
    let (_registries, source, f, target) = round_trip().await;

    for id in [f.root, f.helper] {
        assert_eq!(target.action(id).await, source.action(id).await, "{id}");
    }
    let queues = target.provider.queue_repo().list().await.unwrap();
    let source_queues = source.provider.queue_repo().list().await.unwrap();
    let queue = |queues: &[Queue]| queues.iter().find(|q| q.id == f.queue).cloned();
    assert_eq!(queue(&queues), queue(&source_queues));
    let (script, original) = (
        target.script(SCRIPT_NAME).await.unwrap(),
        source.script(SCRIPT_NAME).await.unwrap(),
    );
    assert_eq!(
        (script.id, script.body, script.enabled),
        (original.id, original.body, original.enabled)
    );
}

#[tokio::test]
async fn importing_into_an_empty_store_keeps_triggers_their_enabled_state_and_their_links() {
    let (_registries, source, f, target) = round_trip().await;

    for id in [f.root_trigger, f.toggled_trigger] {
        assert_eq!(target.trigger(id).await, source.trigger(id).await, "{id}");
    }
    assert!(!target.trigger(f.root_trigger).await.unwrap().enabled);
    assert_eq!(target.linked(f.root).await, vec![f.root_trigger]);
    assert_eq!(target.linked(f.helper).await, Vec::new());
    assert!(target.trigger(f.helper_trigger).await.is_none());
}

#[tokio::test]
async fn importing_a_global_brings_its_definition_with_the_type_default_not_the_live_value() {
    let (_registries, _source, _f, target) = round_trip().await;

    let p = target.provider.as_ref();
    assert_eq!(
        (
            GlobalsRepo::get(p, GLOBAL_NAME).await.unwrap(),
            GlobalsRepo::persisted(p, GLOBAL_NAME).await.unwrap(),
        ),
        (Some(Variant::Int(0)), Some(true))
    );
}

#[tokio::test]
async fn importing_into_an_empty_store_reconnects_clip_media_and_overlay() {
    let (_registries, _source, f, target) = round_trip().await;

    let overlay = target.overlay_named(OVERLAY_NAME).await;
    let clip = target
        .provider
        .soundboard_clips_repo()
        .get(f.clip)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            target.clip_blob(f.clip).await,
            clip.name,
            overlay.config.get("sound").and_then(Variant::as_str),
            overlay.config.get("icon").and_then(Variant::as_str),
            target.provider.media_repo().read(&f.icon).await.unwrap(),
        ),
        (
            Some(f.ding.clone()),
            CLIP_NAME.to_owned(),
            Some(format!("clip:{}", f.clip).as_str()),
            Some(format!("image:{}", f.icon.as_str()).as_str()),
            PNG.to_vec(),
        )
    );
}

#[tokio::test]
async fn importing_an_overlay_copies_its_source_override_file_and_retargets_the_send_step() {
    let (_registries, _source, f, target) = round_trip().await;

    let overlay = target.overlay_named(OVERLAY_NAME).await;
    let root = target.action(f.root).await.unwrap();
    let copied = std::fs::read(
        target
            .overlay_root()
            .join(overlay.id.as_str())
            .join(OVERRIDE_FILE),
    )
    .unwrap();
    assert_eq!(copied, OVERRIDE_BYTES);
    assert_eq!(
        step_value(
            &root,
            forge_runtime::OVERLAY_SEND_KIND_ID,
            forge_runtime::OVERLAY_TARGET_KEY
        ),
        overlay.id.as_str()
    );
}

#[tokio::test]
async fn an_id_match_defaults_to_skip_and_leaves_the_local_copy_untouched() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;
    let bundle = export(&store, &registries, &[f.root]).await;
    store
        .provider
        .action_repo()
        .save(&Action {
            name: "Locally renamed".to_owned(),
            ..store.action(f.root).await.unwrap()
        })
        .await
        .unwrap();

    let (staged, plan) = preview(&store, &registries, &bundle).await;
    let conflicted: Vec<_> = plan
        .items()
        .iter()
        .filter(|item| item.key.kind != PackItemKind::Media)
        .map(|item| (item.conflict.clone(), item.resolution))
        .collect();
    apply(&store, &registries, &staged, plan).await.unwrap();

    assert!(
        conflicted
            .iter()
            .all(|c| *c == (PackConflict::IdMatch, Some(PackResolution::Skip))),
        "{conflicted:?}"
    );
    assert_eq!(store.action(f.root).await.unwrap().name, "Locally renamed");
}

#[tokio::test]
async fn a_name_only_match_defaults_to_keep_both() {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let f = seed(&source).await;
    let bundle = export(&source, &registries, &[f.root]).await;
    let target = Store::open().await;
    seed(&target).await;

    let (_staged, plan) = preview(&target, &registries, &bundle).await;
    let root = plan
        .items()
        .iter()
        .find(|item| item.key.kind == PackItemKind::Action && item.key.id == f.root.to_string())
        .unwrap();

    assert!(matches!(root.conflict, PackConflict::NameMatch { .. }));
    assert_eq!(root.resolution, Some(PackResolution::KeepBoth));
}

#[tokio::test]
async fn setting_a_resolution_on_an_item_without_a_conflict_or_outside_the_pack_is_refused() {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let f = seed(&source).await;
    let bundle = export(&source, &registries, &[f.root]).await;
    let target = Store::open().await;
    let (_staged, mut plan) = preview(&target, &registries, &bundle).await;

    let unconflicted = PackItemKey {
        kind: PackItemKind::Action,
        id: f.root.to_string(),
    };
    let foreign = PackItemKey {
        kind: PackItemKind::Action,
        id: f.unrelated.to_string(),
    };
    assert!(matches!(
        plan.set_resolution(&unconflicted, PackResolution::Replace),
        Err(TransferError::PackResolutionNotAllowed { .. })
    ));
    assert!(matches!(
        plan.set_resolution(&foreign, PackResolution::Replace),
        Err(TransferError::UnknownPackItem { .. })
    ));
}

struct KeptBoth {
    registries: Registries,
    f: Fixture,
    target: Store,
    outcome: PackImportOutcome,
}

impl KeptBoth {
    fn local(&self, kind: PackItemKind, pack_id: &str) -> String {
        local_id(&self.outcome, kind, pack_id)
    }

    async fn new_root(&self) -> Action {
        let id = self.local(PackItemKind::Action, &self.f.root.to_string());
        self.target.action(id.parse().unwrap()).await.unwrap()
    }
}

async fn keep_both_over_a_previous_import() -> KeptBoth {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let f = seed(&source).await;
    let bundle = export(&source, &registries, &[f.root]).await;
    let target = Store::open().await;
    import(&target, &registries, &bundle, defaults)
        .await
        .unwrap();

    let outcome = import(
        &target,
        &registries,
        &bundle,
        on_conflict(PackResolution::KeepBoth),
    )
    .await
    .unwrap();
    KeptBoth {
        registries,
        f,
        target,
        outcome,
    }
}

#[tokio::test]
async fn keep_both_gives_every_copied_item_a_fresh_id_and_a_numbered_name() {
    let kept = keep_both_over_a_previous_import().await;
    let root = kept.new_root().await;
    let helper_id = kept.local(PackItemKind::Action, &kept.f.helper.to_string());
    let helper = kept
        .target
        .action(helper_id.parse().unwrap())
        .await
        .unwrap();

    assert_ne!(root.id, kept.f.root);
    assert_ne!(helper.id, kept.f.helper);
    assert_eq!(
        (root.name.as_str(), helper.name.as_str()),
        ("Root (2)", "Helper (2)")
    );
}

#[tokio::test]
async fn keep_both_rewrites_every_internal_step_reference_to_the_copies() {
    let kept = keep_both_over_a_previous_import().await;
    let root = kept.new_root().await;
    let f = &kept.f;
    let local = |kind, id: String| kept.local(kind, &id);

    assert_eq!(
        vec![
            step_value(&root, "core.action.run", "action_id"),
            step_value(&root, "core.trigger.enable", "trigger_instance_id"),
            step_value(&root, SCRIPT_STEP, "script_name"),
            step_value(&root, "core.globals.set", "name"),
            step_value(&root, "soundboard.sound.play", "clip_id"),
            step_value(
                &root,
                forge_runtime::OVERLAY_SEND_KIND_ID,
                forge_runtime::OVERLAY_TARGET_KEY
            ),
            root.queue_id.to_string(),
        ],
        vec![
            local(PackItemKind::Action, f.helper.to_string()),
            local(PackItemKind::Trigger, f.toggled_trigger.to_string()),
            format!("{SCRIPT_NAME} (2)"),
            format!("{GLOBAL_NAME} (2)"),
            local(PackItemKind::Clip, f.clip.to_string()),
            local(PackItemKind::Overlay, f.overlay.to_string()),
            local(PackItemKind::Queue, f.queue.to_string()),
        ]
    );
}

#[tokio::test]
async fn keep_both_leaves_integration_owned_references_as_exported() {
    let kept = keep_both_over_a_previous_import().await;
    let root = kept.new_root().await;

    assert_eq!(step_value(&root, SCENE_STEP, "scene"), SCENE);
}

#[tokio::test]
async fn keep_both_renames_a_global_with_a_literal_percent_but_never_a_templated_name() {
    const PERCENT_GLOBAL: &str = "50% off";
    const TEMPLATED_GLOBAL: &str = "%slot%";
    let registries = Registries::real().await;
    let source = Store::open().await;
    let p = source.provider.as_ref();
    let queue = Queue {
        id: QueueId::new(),
        name: QUEUE_NAME.to_owned(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::MIN,
    };
    p.queue_repo().save(&queue).await.unwrap();
    for name in [PERCENT_GLOBAL, TEMPLATED_GLOBAL] {
        GlobalsRepo::set(p, name, Variant::Int(1), true)
            .await
            .unwrap();
    }
    let root = action(
        ActionId::new(),
        "Sale",
        queue.id,
        [PERCENT_GLOBAL, TEMPLATED_GLOBAL]
            .map(|name| {
                step(
                    "core.globals.set",
                    config(&[("name", text(name)), ("value", text("2"))]),
                )
            })
            .to_vec(),
    );
    p.action_repo().save(&root).await.unwrap();
    let bundle = export_with(&source, &registries, &[root.id], |selection| {
        selection.globals.insert(TEMPLATED_GLOBAL.to_owned());
    })
    .await;
    let target = Store::open().await;
    import(&target, &registries, &bundle, defaults)
        .await
        .unwrap();

    let outcome = import(
        &target,
        &registries,
        &bundle,
        on_conflict(PackResolution::KeepBoth),
    )
    .await
    .unwrap();
    let copy = local_id(&outcome, PackItemKind::Action, &root.id.to_string());
    let copied = target.action(copy.parse().unwrap()).await.unwrap();
    let names: Vec<_> = copied
        .sub_actions
        .iter()
        .filter_map(|step| step.config.get("name").and_then(Variant::as_str))
        .collect();

    assert_eq!(names, vec!["50% off (2)", TEMPLATED_GLOBAL]);
}

#[tokio::test]
async fn keep_both_links_the_copied_trigger_to_the_copied_action_only() {
    let kept = keep_both_over_a_previous_import().await;
    let root = kept.new_root().await;
    let copied_trigger = kept.local(PackItemKind::Trigger, &kept.f.root_trigger.to_string());

    assert_eq!(
        kept.target
            .linked(root.id)
            .await
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec![copied_trigger]
    );
    assert_eq!(
        kept.target.linked(kept.f.root).await,
        vec![kept.f.root_trigger]
    );
}

#[tokio::test]
async fn keep_both_leaves_no_internal_reference_of_the_copied_graph_dangling() {
    let kept = keep_both_over_a_previous_import().await;
    let catalog = kept.registries.catalog();
    let mut references = Vec::new();
    for item in &kept.outcome.written {
        if item.key.kind == PackItemKind::Action {
            let action = kept.target.action(item.local_id.parse().unwrap()).await;
            references.extend(catalog.action_references(&action.unwrap()).references);
        }
    }
    let overlay = kept
        .target
        .overlay_named(&format!("{OVERLAY_NAME} (2)"))
        .await;
    let clip_ref = overlay
        .config
        .get("sound")
        .and_then(Variant::as_str)
        .and_then(|sound| sound.strip_prefix("clip:"))
        .unwrap()
        .to_owned();

    let mut dangling = Vec::new();
    for reference in references {
        if let ReferenceNamespace::Internal(namespace) = reference.namespace
            && !kept.target.holds(namespace, &reference.value).await
        {
            dangling.push((namespace, reference.value));
        }
    }
    assert_eq!(dangling, Vec::new());
    assert_eq!(
        clip_ref,
        kept.local(PackItemKind::Clip, &kept.f.clip.to_string())
    );
}

#[tokio::test]
async fn skipping_a_name_matched_queue_and_clip_points_a_copied_action_at_the_local_ones() {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let f = seed(&source).await;
    let bundle = export(&source, &registries, &[f.root]).await;
    let target = Store::open().await;
    let local = seed(&target).await;
    let (staged, mut plan) = preview(&target, &registries, &bundle).await;
    let skipped: Vec<_> = plan
        .items()
        .iter()
        .filter(|item| matches!(item.key.kind, PackItemKind::Queue | PackItemKind::Clip))
        .map(|item| (item.key.clone(), item.conflict.clone()))
        .collect();
    for (key, _) in &skipped {
        plan.set_resolution(key, PackResolution::Skip).unwrap();
    }

    let outcome = apply(&target, &registries, &staged, plan).await.unwrap();
    let root = target
        .action(
            local_id(&outcome, PackItemKind::Action, &f.root.to_string())
                .parse()
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        skipped
            .iter()
            .all(|(_, conflict)| matches!(conflict, PackConflict::NameMatch { .. })),
        "{skipped:?}"
    );
    assert_eq!(
        (
            root.queue_id,
            step_value(&root, "soundboard.sound.play", "clip_id"),
        ),
        (local.queue, local.clip.to_string())
    );
}

#[tokio::test]
async fn replacing_writes_a_pre_pack_safety_snapshot_that_restore_can_stage() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;
    let bundle = export(&store, &registries, &[f.root]).await;

    let outcome = import(
        &store,
        &registries,
        &bundle,
        on_conflict(PackResolution::Replace),
    )
    .await
    .unwrap();
    let snapshot = outcome.safety_snapshot.unwrap();
    let staged = stage_full_restore(StageRequest {
        bundle: snapshot.clone(),
        data_dir: store.data_dir().to_path_buf(),
        passphrase: None,
        database_schema: Arc::new(SqliteDatabaseFileSchema),
    })
    .await;

    assert!(
        snapshot
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("forge-pre-pack")
    );
    assert!(staged.is_ok(), "{staged:?}");
}

#[tokio::test]
async fn an_import_that_replaces_nothing_still_writes_a_safety_snapshot() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;
    let bundle = export(&store, &registries, &[f.root]).await;

    let outcome = import(&store, &registries, &bundle, defaults)
        .await
        .unwrap();

    let snapshot = outcome.safety_snapshot.unwrap();
    assert!(snapshot.is_file());
    assert!(
        snapshot
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("forge-pre-pack")
    );
}

#[tokio::test]
async fn replacing_overwrites_local_edits_with_the_pack_version_in_place() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;
    let bundle = export(&store, &registries, &[f.root]).await;
    let exported = store.action(f.root).await.unwrap();
    store
        .provider
        .action_repo()
        .save(&Action {
            name: "Locally renamed".to_owned(),
            sub_actions: Vec::new(),
            ..exported.clone()
        })
        .await
        .unwrap();

    let outcome = import(
        &store,
        &registries,
        &bundle,
        on_conflict(PackResolution::Replace),
    )
    .await
    .unwrap();

    assert_eq!(store.action(f.root).await, Some(exported));
    assert!(
        outcome
            .written
            .iter()
            .any(|item| item.local_id == f.root.to_string() && item.replaced)
    );
}

struct FailingPack {
    _source: Store,
    f: Fixture,
    root_only: PathBuf,
    failing: PathBuf,
}

async fn pack_whose_last_action_lacks_its_queue(registries: &Registries) -> FailingPack {
    let source = Store::open().await;
    let f = seed(&source).await;
    let orphan_queue = Queue {
        id: QueueId::new(),
        name: "Orphan queue".to_owned(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::MIN,
    };
    source
        .provider
        .queue_repo()
        .save(&orphan_queue)
        .await
        .unwrap();
    let late = action(last_action_id(), "Late", orphan_queue.id, Vec::new());
    source.provider.action_repo().save(&late).await.unwrap();
    let root_only = export(&source, registries, &[f.root]).await;
    let failing = export_with(&source, registries, &[f.root, late.id], |selection| {
        selection.queues.remove(&orphan_queue.id);
    })
    .await;
    FailingPack {
        _source: source,
        f,
        root_only,
        failing,
    }
}

fn is_late_without_queue(result: &Result<PackImportOutcome, TransferError>) -> bool {
    matches!(result, Err(TransferError::PackActionWithoutQueue { action }) if action == "Late")
}

#[tokio::test]
async fn a_failure_after_items_were_created_leaves_the_store_as_it_was() {
    let registries = Registries::real().await;
    let pack = pack_whose_last_action_lacks_its_queue(&registries).await;
    let target = Store::open().await;
    let before = state_of(&target).await;

    let failed = import(&target, &registries, &pack.failing, defaults).await;

    assert!(is_late_without_queue(&failed), "{failed:?}");
    assert_eq!(state_of(&target).await, before);
}

#[tokio::test]
async fn a_failure_after_items_were_replaced_restores_every_local_version() {
    let registries = Registries::real().await;
    let pack = pack_whose_last_action_lacks_its_queue(&registries).await;
    let target = Store::open().await;
    import(&target, &registries, &pack.root_only, defaults)
        .await
        .unwrap();
    let p = target.provider.as_ref();
    let root = target.action(pack.f.root).await.unwrap();
    p.action_repo()
        .save(&Action {
            name: "Locally renamed".to_owned(),
            sub_actions: Vec::new(),
            ..root
        })
        .await
        .unwrap();
    let script = target.script(SCRIPT_NAME).await.unwrap();
    ScriptRepo::save(
        p,
        ScriptRecord {
            body: "let edited = 2;".to_owned(),
            ..script
        },
    )
    .await
    .unwrap();
    GlobalsRepo::set(p, GLOBAL_NAME, Variant::Int(41), false)
        .await
        .unwrap();
    let mut overlay = target.overlay_named(OVERLAY_NAME).await;
    overlay.display_name = "Edited overlay".to_owned();
    p.overlay_repo().save(&overlay).await.unwrap();
    std::fs::write(
        target
            .overlay_root()
            .join(overlay.id.as_str())
            .join(OVERRIDE_FILE),
        b"edited css",
    )
    .unwrap();
    let before = state_of(&target).await;

    let failed = import(
        &target,
        &registries,
        &pack.failing,
        on_conflict(PackResolution::Replace),
    )
    .await;

    assert!(is_late_without_queue(&failed), "{failed:?}");
    assert_eq!(state_of(&target).await, before);
}

#[tokio::test]
async fn a_failed_import_keeps_an_archived_action_that_shares_an_imported_id() {
    let registries = Registries::real().await;
    let pack = pack_whose_last_action_lacks_its_queue(&registries).await;
    let target = Store::open().await;
    import(&target, &registries, &pack.root_only, defaults)
        .await
        .unwrap();
    let actions = target.provider.action_repo();
    assert!(actions.archive(pack.f.root).await.unwrap());

    let failed = import(&target, &registries, &pack.failing, defaults).await;

    assert!(is_late_without_queue(&failed), "{failed:?}");
    assert!(
        actions.restore(pack.f.root).await.unwrap(),
        "the archived action was deleted by the rollback"
    );
}

async fn archive_script(store: &Store) -> ScriptId {
    let id = store.script(SCRIPT_NAME).await.unwrap().id;
    assert!(store.provider.archive_script(id).await.unwrap());
    id
}

async fn archived_scripts(store: &Store) -> Vec<ScriptId> {
    store
        .provider
        .list_archived_scripts()
        .await
        .unwrap()
        .into_iter()
        .map(|script| script.id)
        .collect()
}

fn script_item(plan: &PackImportPlan) -> (PackConflict, bool) {
    plan.items()
        .iter()
        .find(|item| item.key.kind == PackItemKind::Script)
        .map(|item| (item.conflict.clone(), item.archived))
        .unwrap()
}

#[tokio::test]
async fn a_pack_script_matching_an_archived_local_script_by_id_is_an_archived_conflict() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;
    let bundle = export(&store, &registries, &[f.root]).await;
    archive_script(&store).await;

    let (_staged, plan) = preview(&store, &registries, &bundle).await;

    assert_eq!(script_item(&plan), (PackConflict::IdMatch, true));
}

#[tokio::test]
async fn a_pack_script_matching_an_archived_local_script_by_name_is_an_archived_conflict() {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let f = seed(&source).await;
    let bundle = export(&source, &registries, &[f.root]).await;
    let target = Store::open().await;
    seed(&target).await;
    let local = archive_script(&target).await;

    let (_staged, plan) = preview(&target, &registries, &bundle).await;

    assert_eq!(
        script_item(&plan),
        (
            PackConflict::NameMatch {
                local_id: local.to_string()
            },
            true
        )
    );
}

#[tokio::test]
async fn replacing_an_archived_local_script_brings_it_back_live_with_the_pack_body() {
    let registries = Registries::real().await;
    let store = Store::open().await;
    let f = seed(&store).await;
    let bundle = export(&store, &registries, &[f.root]).await;
    let local = store.script(SCRIPT_NAME).await.unwrap();
    ScriptRepo::save(
        store.provider.as_ref(),
        ScriptRecord {
            body: "let edited = 2;".to_owned(),
            ..local
        },
    )
    .await
    .unwrap();
    archive_script(&store).await;

    import(
        &store,
        &registries,
        &bundle,
        on_conflict(PackResolution::Replace),
    )
    .await
    .unwrap();

    assert_eq!(
        (
            store.script(SCRIPT_NAME).await.map(|script| script.body),
            archived_scripts(&store).await
        ),
        (Some(SCRIPT_BODY.to_owned()), Vec::new())
    );
}

#[tokio::test]
async fn a_failed_import_that_replaced_an_archived_script_archives_it_again() {
    let registries = Registries::real().await;
    let pack = pack_whose_last_action_lacks_its_queue(&registries).await;
    let target = Store::open().await;
    import(&target, &registries, &pack.root_only, defaults)
        .await
        .unwrap();
    let local = archive_script(&target).await;

    let failed = import(
        &target,
        &registries,
        &pack.failing,
        on_conflict(PackResolution::Replace),
    )
    .await;

    assert!(is_late_without_queue(&failed), "{failed:?}");
    assert_eq!(
        (
            target.script(SCRIPT_NAME).await.map(|script| script.id),
            archived_scripts(&target).await
        ),
        (None, vec![local])
    );
}

#[tokio::test]
async fn applying_a_plan_after_the_conflicts_changed_is_refused_as_stale_without_writing() {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let f = seed(&source).await;
    let bundle = export(&source, &registries, &[f.root]).await;
    let target = Store::open().await;
    let (staged, plan) = preview(&target, &registries, &bundle).await;
    let queue = target.provider.queue_repo().list().await.unwrap()[0].id;
    target
        .provider
        .action_repo()
        .save(&action(ActionId::new(), "Root", queue, Vec::new()))
        .await
        .unwrap();
    let before = state_of(&target).await;

    let stale = apply(&target, &registries, &staged, plan).await;

    assert!(
        matches!(stale, Err(TransferError::PackPlanStale)),
        "{stale:?}"
    );
    assert_eq!(state_of(&target).await, before);
}

#[tokio::test]
async fn the_preview_flags_scripts_http_and_script_steps_and_overlay_sources_as_executable() {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let f = seed(&source).await;
    let bundle = export(&source, &registries, &[f.root]).await;
    let target = Store::open().await;

    let (_staged, plan) = preview(&target, &registries, &bundle).await;

    assert_eq!(
        plan.executable_content()
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            ExecutableContent::Script {
                name: SCRIPT_NAME.to_owned(),
            },
            ExecutableContent::HttpStep {
                action: "Root".to_owned(),
                kind_id: HTTP_STEP.to_owned(),
            },
            ExecutableContent::ScriptStep {
                action: "Root".to_owned(),
                kind_id: SCRIPT_STEP.to_owned(),
            },
            ExecutableContent::OverlaySource {
                overlay: OVERLAY_NAME.to_owned(),
                file: OVERRIDE_FILE.to_owned(),
            },
        ])
    );
}

#[tokio::test]
async fn the_preview_flags_an_http_step_nested_under_a_disabled_branch_as_executable() {
    let registries = Registries::real().await;
    let source = Store::open().await;
    let queue = Queue {
        id: QueueId::new(),
        name: QUEUE_NAME.to_owned(),
        description: "pack queue".to_owned(),
        concurrency: std::num::NonZeroU32::MIN,
    };
    source.provider.queue_repo().save(&queue).await.unwrap();
    let nested_http = Variant::Object(config(&[
        ("kind_id", text(HTTP_STEP)),
        (
            "config",
            Variant::Object(config(&[("url", text("https://example.invalid/hook"))])),
        ),
        ("enabled", Variant::Bool(true)),
    ]));
    let disabled_branch = SubActionStep {
        enabled: false,
        ..step(
            "core.logic.if_then_else",
            config(&[
                ("condition", text("true")),
                ("then_chain", Variant::Array(vec![nested_http])),
            ]),
        )
    };
    let nested = action(ActionId::new(), "Nested", queue.id, vec![disabled_branch]);
    source.provider.action_repo().save(&nested).await.unwrap();
    let bundle = export(&source, &registries, &[nested.id]).await;
    let target = Store::open().await;

    let (_staged, plan) = preview(&target, &registries, &bundle).await;

    assert_eq!(
        plan.executable_content(),
        [ExecutableContent::HttpStep {
            action: "Nested".to_owned(),
            kind_id: HTTP_STEP.to_owned(),
        }]
    );
}
