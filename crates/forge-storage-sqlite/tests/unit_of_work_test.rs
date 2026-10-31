#![allow(clippy::unwrap_used)]
mod common;
use std::path::PathBuf;

use forge_storage::settings::core_settings;
use forge_storage::{
    DataProvider, MediaBlobId, NewOverlay, OverlayConfig, OverlayDefinition, OverlayId,
    ScriptRecord, StorageError, StoreChange, StoredClip, TriggerWiring, clip_source_referrer,
    setting_row,
};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ClipId, DEFAULT_QUEUE_ID, OutputDevice, OverlayCredential, PermissionRung, Queue,
    QueueId, ScriptContract, ScriptId, TriggerConfig, TriggerInstance, TriggerInstanceId, Variant,
};
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;
use time::OffsetDateTime;

const OGG: &[u8] = b"OggS\x00\x02\x00\x00unit of work";
const TRACKED_TABLES: [&str; 8] = [
    "queues",
    "scripts",
    "globals",
    "actions",
    "trigger_instances",
    "action_trigger_instances",
    "settings",
    "overlays",
];

struct Store {
    backend: SqliteBackend,
    side: SqlitePool,
    _dir: TempDir,
}

async fn open() -> Store {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", dir.path().join("test.db").display());
    let backend = SqliteBackend::open_for_test(&url, [0xab; 32], dir.path().join("media"), None)
        .await
        .unwrap();
    let side = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    Store {
        backend,
        side,
        _dir: dir,
    }
}

impl Store {
    async fn row_counts(&self) -> Vec<(&'static str, i64)> {
        let mut counts = Vec::new();
        for table in TRACKED_TABLES {
            let count: i64 =
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                    .fetch_one(&self.side)
                    .await
                    .unwrap();
            counts.push((table, count));
        }
        counts
    }

    async fn refuse(&self, statement: &str, table: &str) {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE TRIGGER refuse_{table} BEFORE {statement} ON {table}
             BEGIN SELECT RAISE(ABORT, 'refused'); END"
        )))
        .execute(&self.side)
        .await
        .unwrap();
    }

    async fn seed_action(&self) -> Action {
        let action = action(DEFAULT_QUEUE_ID);
        self.backend.action_repo().save(&action).await.unwrap();
        action
    }

    async fn commit_overlay_wiring(&self, wiring: &TriggerWiring) -> Result<(), StorageError> {
        self.backend
            .trigger_instance_repo()
            .commit_overlay_wiring(
                wiring,
                self.backend.queue_repo(),
                self.backend.action_repo(),
            )
            .await
    }

    async fn stored_blob(&self) -> MediaBlobId {
        self.backend
            .media_repo()
            .store("ding.ogg", OGG.to_vec())
            .await
            .unwrap()
            .id
    }
}

fn queue() -> Queue {
    Queue {
        id: QueueId::new(),
        name: "alerts".to_owned(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::MIN,
    }
}

fn action(queue_id: QueueId) -> Action {
    common::action_named("alert", queue_id)
}

fn instance() -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: "twitch.follow".to_owned(),
        name: "follow".to_owned(),
        overrides: TriggerConfig::new(),
        enabled: true,
        user_defined: true,
        platform_scope: Default::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    }
}

fn script() -> ScriptRecord {
    let now = OffsetDateTime::now_utc();
    ScriptRecord {
        id: ScriptId::new(),
        name: "greet".to_owned(),
        body: "let x = 1;".to_owned(),
        contract: ScriptContract::default(),
        enabled: true,
        created_at: now,
        last_modified: now,
    }
}

fn clip() -> StoredClip {
    StoredClip {
        id: ClipId::new(),
        name: "airhorn".to_owned(),
        file_path: PathBuf::from("/clips/airhorn.ogg"),
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

fn missing_overlay() -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new("nowhere"),
        display_name: "Nowhere".to_owned(),
        kind_id: "overlay.alert".to_owned(),
        enabled: true,
        position: 0,
        config: OverlayConfig::new(),
        config_schema_version: 1,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new("2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d"),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn new_overlay(id: &str) -> NewOverlay {
    NewOverlay {
        id: OverlayId::new(id),
        display_name: id.to_owned(),
        kind_id: "overlay.alert".to_owned(),
        enabled: true,
        position: None,
        config: OverlayConfig::new(),
        config_schema_version: 1,
        source_overrides: Vec::new(),
    }
}

fn wiring() -> TriggerWiring {
    let queue = queue();
    TriggerWiring {
        action: action(queue.id),
        created_queue: Some(queue),
        instance: instance(),
    }
}

impl Store {
    async fn with_alert_queue_and_another() -> (Store, QueueId, QueueId) {
        let store = open().await;
        let (holder, other) = (queue(), queue());
        let (holder_id, other_id) = (holder.id, other.id);
        store
            .backend
            .commit_changes(&[
                StoreChange::SaveQueue(holder),
                StoreChange::AssignOverlayAlertQueue(holder_id),
                StoreChange::SaveQueue(other),
            ])
            .await
            .unwrap();
        (store, holder_id, other_id)
    }

    async fn role_holders(&self) -> Vec<String> {
        sqlx::query_scalar("SELECT id FROM queues WHERE role IS NOT NULL")
            .fetch_all(&self.side)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn assigning_the_alert_role_moves_it_off_the_queue_holding_it() {
    let (store, _holder, other) = Store::with_alert_queue_and_another().await;

    store
        .backend
        .commit_changes(&[StoreChange::AssignOverlayAlertQueue(other)])
        .await
        .unwrap();

    assert_eq!(store.role_holders().await, [other.to_string()]);
}

#[tokio::test]
async fn assigning_the_alert_role_to_its_holder_keeps_it_there() {
    let (store, holder, _other) = Store::with_alert_queue_and_another().await;

    store
        .backend
        .commit_changes(&[StoreChange::AssignOverlayAlertQueue(holder)])
        .await
        .unwrap();

    assert_eq!(store.role_holders().await, [holder.to_string()]);
}

#[tokio::test]
async fn claiming_a_taken_alert_role_changes_nothing() {
    let (store, holder, other) = Store::with_alert_queue_and_another().await;

    store
        .backend
        .commit_changes(&[StoreChange::ClaimOverlayAlertQueue(other)])
        .await
        .unwrap();

    assert_eq!(store.role_holders().await, [holder.to_string()]);
}

#[tokio::test]
async fn claiming_a_free_alert_role_takes_it() {
    let store = open().await;
    let claimant = queue();
    let id = claimant.id;

    store
        .backend
        .commit_changes(&[
            StoreChange::SaveQueue(claimant),
            StoreChange::ClaimOverlayAlertQueue(id),
        ])
        .await
        .unwrap();

    assert_eq!(store.role_holders().await, [id.to_string()]);
}

#[tokio::test]
async fn a_failure_mid_batch_leaves_every_other_change_unwritten() {
    let store = open().await;
    let before = store.row_counts().await;
    let queue = queue();
    let action = action(queue.id);
    let trigger = instance();

    let failed = store
        .backend
        .commit_changes(&[
            StoreChange::SaveQueue(queue),
            StoreChange::SaveScript(Box::new(script())),
            StoreChange::SetGlobal {
                name: "score".to_owned(),
                value: Variant::Int(1),
                persisted: true,
            },
            StoreChange::SaveAction(Box::new(action.clone())),
            StoreChange::SaveTrigger(Box::new(trigger.clone())),
            StoreChange::LinkTrigger {
                action: action.id,
                trigger: trigger.id,
            },
            StoreChange::PutSetting(
                setting_row(&core_settings::EVENT_LOG_RETENTION_DAYS, &3).unwrap(),
            ),
            StoreChange::CreateOverlay(Box::new(new_overlay("first"))),
            StoreChange::SaveOverlay(Box::new(missing_overlay())),
            StoreChange::CreateOverlay(Box::new(new_overlay("after-the-failure"))),
        ])
        .await;

    assert!(
        matches!(failed, Err(StorageError::NotFound { .. })),
        "{failed:?}"
    );
    assert_eq!(store.row_counts().await, before);
}

#[tokio::test]
async fn a_delete_that_precedes_the_failure_comes_back() {
    let store = open().await;
    let seeded = store.seed_action().await;

    let failed = store
        .backend
        .commit_changes(&[
            StoreChange::DeleteAction(seeded.id),
            StoreChange::DeleteQueue(DEFAULT_QUEUE_ID),
        ])
        .await;

    assert!(
        matches!(failed, Err(StorageError::DefaultQueueUndeletable)),
        "{failed:?}"
    );
    assert!(
        store
            .backend
            .action_repo()
            .get(seeded.id)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn a_batch_advances_the_catalog_revision_once_however_many_changes_it_holds() {
    let store = open().await;
    let queue = queue();
    let action = action(queue.id);
    let revision = store.backend.catalog_revision();
    let before = revision.current();

    store
        .backend
        .commit_changes(&[
            StoreChange::SaveQueue(queue),
            StoreChange::SaveAction(Box::new(action)),
            StoreChange::SaveTrigger(Box::new(instance())),
            StoreChange::SaveScript(Box::new(script())),
        ])
        .await
        .unwrap();

    assert_eq!(revision.current(), before + 1);
}

#[tokio::test]
async fn a_batch_outside_the_catalog_leaves_the_catalog_revision_alone() {
    let store = open().await;
    let revision = store.backend.catalog_revision();
    let before = revision.current();

    store
        .backend
        .commit_changes(&[StoreChange::SetGlobal {
            name: "score".to_owned(),
            value: Variant::Int(1),
            persisted: true,
        }])
        .await
        .unwrap();

    assert_eq!(revision.current(), before);
}

#[tokio::test]
async fn deleting_an_action_in_a_batch_advances_the_scheduled_run_revision_once() {
    let store = open().await;
    let first = store.seed_action().await;
    let second = store.seed_action().await;
    let revision = store.backend.scheduled_run_revision();
    let before = revision.current();

    store
        .backend
        .commit_changes(&[
            StoreChange::DeleteAction(first.id),
            StoreChange::DeleteAction(second.id),
        ])
        .await
        .unwrap();

    assert_eq!(revision.current(), before + 1);
}

#[tokio::test]
async fn the_scheduled_run_revision_stays_put_unless_an_action_really_went() {
    let store = open().await;
    let seeded = store.seed_action().await;
    let revision = store.backend.scheduled_run_revision();
    let before = revision.current();

    store
        .backend
        .commit_changes(&[StoreChange::SaveAction(Box::new(seeded.clone()))])
        .await
        .unwrap();
    let refused = store
        .backend
        .commit_changes(&[
            StoreChange::DeleteAction(seeded.id),
            StoreChange::DeleteQueue(DEFAULT_QUEUE_ID),
        ])
        .await;

    assert!(refused.is_err());
    assert_eq!(revision.current(), before);
}

#[tokio::test]
async fn overlays_created_in_one_batch_take_consecutive_positions() {
    let store = open().await;

    store
        .backend
        .commit_changes(&[
            StoreChange::CreateOverlay(Box::new(new_overlay("first"))),
            StoreChange::CreateOverlay(Box::new(new_overlay("second"))),
        ])
        .await
        .unwrap();

    let mut positions: Vec<i64> = store
        .backend
        .overlay_repo()
        .list()
        .await
        .unwrap()
        .iter()
        .map(|overlay| overlay.position)
        .collect();
    positions.sort_unstable();
    assert_eq!(positions, [0, 1]);
}

#[tokio::test]
async fn linking_the_same_trigger_twice_in_a_batch_lands_one_link() {
    let store = open().await;
    let action = store.seed_action().await;
    let trigger = instance();
    let link = || StoreChange::LinkTrigger {
        action: action.id,
        trigger: trigger.id,
    };

    store
        .backend
        .commit_changes(&[
            StoreChange::SaveTrigger(Box::new(trigger.clone())),
            link(),
            link(),
        ])
        .await
        .unwrap();

    let linked = store
        .backend
        .trigger_instance_repo()
        .list_for_action(action.id)
        .await
        .unwrap();
    assert_eq!(linked.len(), 1);
}

#[tokio::test]
async fn deleting_a_linked_trigger_in_a_batch_takes_its_links_with_it() {
    let store = open().await;
    let action = store.seed_action().await;
    let trigger = instance();
    let repo = store.backend.trigger_instance_repo();
    repo.save(&trigger).await.unwrap();
    repo.link_action(action.id, trigger.id).await.unwrap();

    store
        .backend
        .commit_changes(&[StoreChange::DeleteTrigger(trigger.id)])
        .await
        .unwrap();

    assert_eq!(repo.actions_using(trigger.id).await.unwrap(), []);
    assert!(repo.get(trigger.id).await.unwrap().is_none());
}

#[tokio::test]
async fn an_overlay_wiring_refused_at_any_write_leaves_nothing_behind() {
    for table in [
        "queues",
        "actions",
        "trigger_instances",
        "action_trigger_instances",
    ] {
        let store = open().await;
        let before = store.row_counts().await;
        store.refuse("INSERT", table).await;

        let failed = store.commit_overlay_wiring(&wiring()).await;

        assert!(failed.is_err(), "{table}");
        assert_eq!(store.row_counts().await, before, "{table}");
    }
}

#[tokio::test]
async fn a_committed_overlay_wiring_reaches_the_catalog_as_one_revision() {
    let store = open().await;
    let revision = store.backend.catalog_revision();
    let before = revision.current();

    store.commit_overlay_wiring(&wiring()).await.unwrap();

    assert_eq!(revision.current(), before + 1);
}

#[tokio::test]
async fn enabling_a_list_holding_an_unknown_trigger_toggles_none_of_them() {
    let store = open().await;
    let known = instance();
    let repo = store.backend.trigger_instance_repo();
    repo.save(&known).await.unwrap();

    let failed = repo
        .set_all_enabled(&[known.id, TriggerInstanceId::new()], false)
        .await;

    assert!(
        matches!(failed, Err(StorageError::NotFound { .. })),
        "{failed:?}"
    );
    assert!(repo.get(known.id).await.unwrap().unwrap().enabled);
}

#[tokio::test]
async fn a_clip_whose_source_reference_is_refused_is_not_stored() {
    let store = open().await;
    let blob = store.stored_blob().await;
    let clip = clip();
    store.refuse("INSERT", "media_references").await;
    let clips = store.backend.soundboard_clips_repo();

    let failed = clips
        .save_with_source(&clip, &blob, store.backend.media_repo())
        .await;

    assert!(failed.is_err());
    assert_eq!(clips.get(clip.id).await.unwrap(), None);
}

#[tokio::test]
async fn a_clip_naming_a_source_the_library_does_not_hold_is_not_stored() {
    let store = open().await;
    let clip = clip();
    let clips = store.backend.soundboard_clips_repo();
    let unknown = MediaBlobId::from_stored(format!("sha256-{}", "ab".repeat(32))).unwrap();

    let failed = clips
        .save_with_source(&clip, &unknown, store.backend.media_repo())
        .await;

    assert!(
        matches!(failed, Err(StorageError::NotFound { .. })),
        "{failed:?}"
    );
    assert_eq!(clips.get(clip.id).await.unwrap(), None);
}

#[tokio::test]
async fn a_clip_that_cannot_be_deleted_keeps_its_source_reference() {
    let store = open().await;
    let blob = store.stored_blob().await;
    let clip = clip();
    let clips = store.backend.soundboard_clips_repo();
    let media = store.backend.media_repo();
    clips
        .save_with_source(&clip, &blob, media.clone())
        .await
        .unwrap();
    store.refuse("DELETE", "soundboard_clips").await;

    let failed = clips.delete_with_sources(clip.id, media.clone()).await;

    assert!(failed.is_err());
    assert_eq!(
        media.blob_of(&clip_source_referrer(clip.id)).await.unwrap(),
        Some(blob)
    );
}

#[tokio::test]
async fn deleting_a_clip_with_its_sources_removes_the_row_and_the_reference() {
    let store = open().await;
    let blob = store.stored_blob().await;
    let clip = clip();
    let clips = store.backend.soundboard_clips_repo();
    let media = store.backend.media_repo();
    clips
        .save_with_source(&clip, &blob, media.clone())
        .await
        .unwrap();

    let deleted = clips
        .delete_with_sources(clip.id, media.clone())
        .await
        .unwrap();

    assert!(deleted);
    assert_eq!(clips.get(clip.id).await.unwrap(), None);
    assert_eq!(
        media.blob_of(&clip_source_referrer(clip.id)).await.unwrap(),
        None
    );
}
