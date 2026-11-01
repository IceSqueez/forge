mod common;

use std::num::NonZeroU32;
use std::sync::Arc;

use common::{Store, action, at, saved_action, store, trigger};
use forge_storage::{
    ActionRepo, DataProvider, OverlayConfig, OverlayId, OverlayRepo, QueueRepo, ScriptRecord,
    ScriptRepo, StorageError, TriggerInstanceRepo,
};
use forge_types::{
    ActionId, DEFAULT_QUEUE_ID, Queue, QueueId, ScriptContract, ScriptId, TriggerInstanceId,
};

fn actions(store: &Store) -> Arc<dyn ActionRepo> {
    store.backend.action_repo()
}

fn triggers(store: &Store) -> Arc<dyn TriggerInstanceRepo> {
    store.backend.trigger_instance_repo()
}

fn queues(store: &Store) -> Arc<dyn QueueRepo> {
    store.backend.queue_repo()
}

fn scripts(store: &Store) -> Arc<dyn ScriptRepo> {
    store.backend.script_repo()
}

fn overlays(store: &Store) -> Arc<dyn OverlayRepo> {
    store.backend.overlay_repo()
}

fn script(name: &str) -> ScriptRecord {
    ScriptRecord {
        id: ScriptId::new(),
        name: name.to_owned(),
        body: "1 + 1".to_owned(),
        contract: ScriptContract::default(),
        enabled: true,
        created_at: at(0),
        last_modified: at(0),
    }
}

fn queue(name: &str) -> Queue {
    Queue {
        id: QueueId::new(),
        name: name.to_owned(),
        description: String::new(),
        concurrency: NonZeroU32::MIN,
    }
}

#[tokio::test]
async fn every_action_write_on_a_missing_id_reports_that_nothing_matched() {
    let store = store().await;
    let repo = actions(&store);
    let missing = ActionId::new();

    assert!(repo.get(missing).await.unwrap().is_none());
    assert!(!repo.set_enabled(missing, true).await.unwrap());
    assert_eq!(repo.toggle_enabled(missing).await.unwrap(), None);
    assert!(!repo.delete(missing).await.unwrap());
    assert!(!repo.archive(missing).await.unwrap());
    assert!(!repo.restore(missing).await.unwrap());
}

#[tokio::test]
async fn every_trigger_write_on_a_missing_id_reports_that_nothing_matched() {
    let store = store().await;
    let repo = triggers(&store);
    let missing = TriggerInstanceId::new();

    assert!(repo.get(missing).await.unwrap().is_none());
    assert!(!repo.set_enabled(missing, true).await.unwrap());
    assert_eq!(repo.toggle_enabled(missing).await.unwrap(), None);
    assert!(!repo.delete(missing).await.unwrap());
    assert!(!repo.archive(missing).await.unwrap());
    assert!(!repo.restore(missing).await.unwrap());
    assert!(!repo.unlink_action(ActionId::new(), missing).await.unwrap());
}

#[tokio::test]
async fn every_script_write_on_a_missing_id_reports_that_nothing_matched() {
    let store = store().await;
    let repo = scripts(&store);
    let missing = ScriptId::new();

    assert!(repo.get(missing).await.unwrap().is_none());
    assert!(repo.get_by_name("absent").await.unwrap().is_none());
    assert!(!repo.delete(missing).await.unwrap());
    assert!(!repo.archive_script(missing).await.unwrap());
    assert!(!repo.restore_script(missing).await.unwrap());
}

#[tokio::test]
async fn every_queue_write_on_a_missing_id_reports_that_nothing_matched() {
    let store = store().await;
    let repo = queues(&store);
    let missing = QueueId::new();

    assert!(repo.get(missing).await.unwrap().is_none());
    assert!(repo.get_by_name("absent").await.unwrap().is_none());
    assert!(!repo.delete(missing).await.unwrap());
    assert_eq!(
        repo.delete_reassigning_actions(missing).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn every_overlay_write_on_a_missing_id_reports_that_nothing_matched() {
    let store = store().await;
    let repo = overlays(&store);
    let missing = OverlayId::new("absent");

    assert!(repo.get(&missing).await.unwrap().is_none());
    assert!(!repo.set_enabled(&missing, true).await.unwrap());
    assert!(!repo.delete(&missing).await.unwrap());
    assert!(!repo.rename(&missing, "x").await.unwrap());
    assert!(
        !repo
            .update_config(&missing, &OverlayConfig::default())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn saving_an_existing_id_replaces_the_row_in_every_catalog_repo() {
    let store = store().await;

    let mut first_action = action("before");
    actions(&store).save(&first_action).await.unwrap();
    "after".clone_into(&mut first_action.name);
    actions(&store).save(&first_action).await.unwrap();
    assert_eq!(actions(&store).list().await.unwrap(), vec![first_action]);

    let mut first_trigger = trigger("before");
    triggers(&store).save(&first_trigger).await.unwrap();
    "after".clone_into(&mut first_trigger.name);
    triggers(&store).save(&first_trigger).await.unwrap();
    assert_eq!(
        triggers(&store)
            .get(first_trigger.id)
            .await
            .unwrap()
            .map(|saved| saved.name),
        Some("after".to_owned())
    );

    let mut first_script = script("before");
    scripts(&store).save(first_script.clone()).await.unwrap();
    "after".clone_into(&mut first_script.name);
    scripts(&store).save(first_script.clone()).await.unwrap();
    assert_eq!(
        scripts(&store)
            .list()
            .await
            .unwrap()
            .into_iter()
            .map(|saved| saved.name)
            .collect::<Vec<_>>(),
        vec!["after".to_owned()]
    );
}

#[tokio::test]
async fn an_archived_action_leaves_every_live_read_until_restored() {
    let store = store().await;
    let repo = actions(&store);
    let id = saved_action(&store, "kept").await;

    assert!(repo.archive(id).await.unwrap());

    assert!(!repo.archive(id).await.unwrap());
    assert!(repo.get(id).await.unwrap().is_none());
    assert!(repo.list().await.unwrap().is_empty());
    assert_eq!(repo.count().await.unwrap(), 0);
    assert!(repo.ids_named("kept").await.unwrap().is_empty());
    assert!(!repo.set_enabled(id, false).await.unwrap());
    assert_eq!(repo.list_archived().await.unwrap().len(), 1);

    assert!(repo.restore(id).await.unwrap());

    assert!(!repo.restore(id).await.unwrap());
    assert_eq!(repo.ids_named("kept").await.unwrap(), vec![id]);
    assert!(repo.list_archived().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_archived_trigger_leaves_every_live_read_until_restored() {
    let store = store().await;
    let repo = triggers(&store);
    let instance = trigger("kept");
    repo.save(&instance).await.unwrap();

    assert!(repo.archive(instance.id).await.unwrap());

    assert!(!repo.archive(instance.id).await.unwrap());
    assert!(repo.get(instance.id).await.unwrap().is_none());
    assert!(repo.list_all().await.unwrap().is_empty());
    assert!(repo.list_user_defined().await.unwrap().is_empty());
    assert_eq!(repo.list_archived().await.unwrap().len(), 1);

    assert!(repo.restore(instance.id).await.unwrap());

    assert!(!repo.restore(instance.id).await.unwrap());
    assert!(repo.get(instance.id).await.unwrap().is_some());
    assert!(repo.list_archived().await.unwrap().is_empty());
}

#[tokio::test]
async fn enabling_an_archived_trigger_reports_that_nothing_matched_like_an_archived_action() {
    let store = store().await;
    let repo = triggers(&store);
    let instance = trigger("kept");
    repo.save(&instance).await.unwrap();
    assert!(repo.archive(instance.id).await.unwrap());

    assert!(!repo.set_enabled(instance.id, false).await.unwrap());
}

#[tokio::test]
async fn an_archived_script_leaves_every_live_read_until_restored() {
    let store = store().await;
    let repo = scripts(&store);
    let record = script("kept");
    repo.save(record.clone()).await.unwrap();

    assert!(repo.archive_script(record.id).await.unwrap());

    assert!(!repo.archive_script(record.id).await.unwrap());
    assert!(repo.get(record.id).await.unwrap().is_none());
    assert!(repo.get_by_name("kept").await.unwrap().is_none());
    assert!(repo.list().await.unwrap().is_empty());
    assert!(repo.list_enabled().await.unwrap().is_empty());
    assert_eq!(repo.list_archived_scripts().await.unwrap().len(), 1);

    assert!(repo.restore_script(record.id).await.unwrap());

    assert!(!repo.restore_script(record.id).await.unwrap());
    assert!(repo.get(record.id).await.unwrap().is_some());
}

#[tokio::test]
async fn toggling_reports_the_new_state_for_actions_and_triggers() {
    let store = store().await;
    let id = saved_action(&store, "flip").await;
    let instance = trigger("flip");
    triggers(&store).save(&instance).await.unwrap();

    assert_eq!(
        actions(&store).toggle_enabled(id).await.unwrap(),
        Some(false)
    );
    assert_eq!(
        actions(&store).toggle_enabled(id).await.unwrap(),
        Some(true)
    );
    assert_eq!(
        triggers(&store).toggle_enabled(instance.id).await.unwrap(),
        Some(false)
    );
    assert_eq!(
        triggers(&store).toggle_enabled(instance.id).await.unwrap(),
        Some(true)
    );
}

#[tokio::test]
async fn an_archived_action_no_longer_counts_as_using_its_trigger() {
    let store = store().await;
    let repo = triggers(&store);
    let live = saved_action(&store, "live").await;
    let archived = saved_action(&store, "archived").await;
    let instance = trigger("shared");
    repo.save(&instance).await.unwrap();
    repo.link_action(live, instance.id).await.unwrap();
    repo.link_action(archived, instance.id).await.unwrap();

    assert!(actions(&store).archive(archived).await.unwrap());

    assert_eq!(repo.actions_using(instance.id).await.unwrap(), vec![live]);
}

#[tokio::test]
async fn enabled_scripts_are_the_enabled_subset_of_the_list() {
    let store = store().await;
    let repo = scripts(&store);
    let on = script("on");
    let off = ScriptRecord {
        enabled: false,
        ..script("off")
    };
    repo.save(on.clone()).await.unwrap();
    repo.save(off).await.unwrap();

    let enabled: Vec<ScriptId> = repo
        .list_enabled()
        .await
        .unwrap()
        .into_iter()
        .map(|record| record.id)
        .collect();

    assert_eq!(repo.list().await.unwrap().len(), 2);
    assert_eq!(enabled, vec![on.id]);
}

#[tokio::test]
async fn deleting_a_queue_with_reassignment_moves_its_actions_to_the_default_queue() {
    let store = store().await;
    let repo = queues(&store);
    let doomed = queue("doomed");
    repo.save(&doomed).await.unwrap();
    let mut held = action("held");
    held.queue_id = doomed.id;
    actions(&store).save(&held).await.unwrap();

    assert_eq!(
        repo.delete_reassigning_actions(doomed.id).await.unwrap(),
        Some(1)
    );

    assert!(repo.get(doomed.id).await.unwrap().is_none());
    assert_eq!(
        actions(&store)
            .get(held.id)
            .await
            .unwrap()
            .map(|a| a.queue_id),
        Some(DEFAULT_QUEUE_ID)
    );
}

#[tokio::test]
async fn a_created_overlay_is_reachable_by_id_and_by_its_credential() {
    let store = store().await;
    let repo = overlays(&store);

    let created = repo.create("Alerts", "overlay.alert", 1).await.unwrap();

    assert_eq!(
        repo.get(&created.id).await.unwrap().map(|o| o.display_name),
        Some("Alerts".to_owned())
    );
    assert_eq!(
        repo.get_by_credential(&created.credential)
            .await
            .unwrap()
            .map(|o| o.id),
        Some(created.id)
    );
}

#[tokio::test]
async fn reissuing_overlay_credentials_retires_the_old_one() {
    let store = store().await;
    let repo = overlays(&store);
    let created = repo.create("Alerts", "overlay.alert", 1).await.unwrap();

    assert_eq!(
        repo.reissue_all_credentials().await.unwrap(),
        vec![created.id.clone()]
    );

    assert!(
        repo.get_by_credential(&created.credential)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn retained_content_of_a_missing_overlay_is_absent_and_cannot_be_written() {
    let store = store().await;
    let repo = overlays(&store);
    let missing = OverlayId::new("absent");

    assert_eq!(repo.get_retained_content(&missing).await.unwrap(), None);
    assert!(matches!(
        repo.set_retained_content(&missing, &OverlayConfig::default())
            .await,
        Err(StorageError::NotFound { .. })
    ));
}
