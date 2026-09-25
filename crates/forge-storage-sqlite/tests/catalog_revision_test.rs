#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::{CatalogRevision, DataProvider, ExecutionStatus};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, PermissionRung, Queue, QueueId, TriggerConfig, TriggerInstance,
    TriggerInstanceId,
};
use time::OffsetDateTime;

mod common;
use common::Sandboxed;

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup() -> Sandboxed<SqliteBackend> {
    common::sandboxed_backend("sqlite::memory:", TEST_KEY).await
}

fn action(queue_id: QueueId) -> Action {
    Action {
        id: ActionId::new(),
        name: "alert".to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: forge_types::ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![],
    }
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

fn queue() -> Queue {
    Queue {
        id: QueueId::new(),
        name: "alerts".to_owned(),
        description: String::new(),
        concurrency: 1,
    }
}

macro_rules! advances {
    ($revision:expr, $label:literal, $op:expr) => {{
        let before = $revision.current();
        $op.await.expect($label);
        assert!(
            $revision.current() > before,
            "{} must advance the catalog revision",
            $label,
        );
    }};
}

macro_rules! holds {
    ($revision:expr, $label:literal, $op:expr) => {{
        let before = $revision.current();
        $op.await.expect($label);
        assert_eq!(
            $revision.current(),
            before,
            "{} must not advance the catalog revision",
            $label,
        );
    }};
}

#[tokio::test]
async fn every_catalog_write_through_the_provider_advances_its_revision() {
    let backend = setup().await;
    let revision: CatalogRevision = backend.catalog_revision();
    let actions = backend.action_repo();
    let instances = backend.trigger_instance_repo();
    let queues = backend.queue_repo();

    let q = queue();
    advances!(revision, "queue save", queues.save(&q));
    let a = action(q.id);
    advances!(revision, "action save", actions.save(&a));
    advances!(
        revision,
        "action set_enabled",
        actions.set_enabled(a.id, false)
    );
    advances!(
        revision,
        "action toggle_enabled",
        actions.toggle_enabled(a.id)
    );
    let copy = ActionId::new();
    advances!(
        revision,
        "action duplicate",
        actions.duplicate(a.id, copy, "copy")
    );
    advances!(revision, "action archive", actions.archive(copy));
    advances!(revision, "action restore", actions.restore(copy));
    advances!(revision, "action delete", actions.delete(copy));

    let i = instance();
    advances!(revision, "instance save", instances.save(&i));
    advances!(
        revision,
        "instance link",
        instances.link_action(a.id, i.id, 0)
    );
    advances!(
        revision,
        "instance set_enabled",
        instances.set_enabled(i.id, false)
    );
    advances!(
        revision,
        "instance unlink",
        instances.unlink_action(a.id, i.id)
    );
    advances!(revision, "instance archive", instances.archive(i.id));
    advances!(revision, "instance restore", instances.restore(i.id));
    advances!(revision, "instance delete", instances.delete(i.id));
    advances!(
        revision,
        "instance upsert_default",
        instances.upsert_default("twitch.raid", "raid")
    );

    advances!(revision, "action delete", actions.delete(a.id));
    advances!(revision, "queue delete", queues.delete(q.id));
}

#[tokio::test]
async fn reads_and_execution_telemetry_leave_the_revision_unchanged() {
    let backend = setup().await;
    let revision = backend.catalog_revision();
    let actions = backend.action_repo();
    let instances = backend.trigger_instance_repo();
    let queues = backend.queue_repo();
    let q = queue();
    queues.save(&q).await.unwrap();
    let a = action(q.id);
    actions.save(&a).await.unwrap();
    let i = instance();
    instances.save(&i).await.unwrap();
    instances.link_action(a.id, i.id, 0).await.unwrap();

    holds!(
        revision,
        "action record_execution",
        actions.record_execution(a.id, OffsetDateTime::now_utc(), 5, ExecutionStatus::Success)
    );
    holds!(
        revision,
        "action prune_executions_before",
        actions.prune_executions_before(OffsetDateTime::now_utc())
    );
    holds!(revision, "action list", actions.list());
    holds!(revision, "action get", actions.get(a.id));
    holds!(revision, "action telemetry", actions.telemetry(a.id));
    holds!(revision, "instance list_all", instances.list_all());
    holds!(
        revision,
        "instance list_for_action",
        instances.list_for_action(a.id)
    );
    holds!(
        revision,
        "instance actions_using",
        instances.actions_using(i.id)
    );
    holds!(revision, "queue list", queues.list());
    holds!(revision, "queue get_by_name", queues.get_by_name("alerts"));
}

#[tokio::test]
async fn duplicating_through_the_provider_keeps_the_trigger_links() {
    let backend = setup().await;
    let q = queue();
    backend.queue_repo().save(&q).await.unwrap();
    let a = action(q.id);
    backend.action_repo().save(&a).await.unwrap();
    let i = instance();
    let instances = backend.trigger_instance_repo();
    instances.save(&i).await.unwrap();
    instances.link_action(a.id, i.id, 0).await.unwrap();

    let copy = ActionId::new();
    backend
        .action_repo()
        .duplicate(a.id, copy, "copy")
        .await
        .unwrap();

    let linked: Vec<_> = instances
        .list_for_action(copy)
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(
        linked,
        vec![i.id],
        "the revision decorator must forward to the backend's link-copying duplicate",
    );
}
