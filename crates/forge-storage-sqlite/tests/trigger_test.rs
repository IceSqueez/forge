#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Action, ActionId, Trigger, TriggerId, TriggerKind};
use std::collections::BTreeMap;

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup() -> SqliteBackend {
    SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open")
}

async fn insert_action(backend: &SqliteBackend) -> ActionId {
    let queue = backend
        .queue_repo()
        .get_by_name("Default")
        .await
        .expect("get default queue")
        .expect("default queue exists");

    let action = Action {
        id: ActionId::new(),
        name: format!("action_{}", ActionId::new()),
        group: None,
        queue_id: queue.id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        description: None,
        sub_actions: vec![],
    };
    let id = action.id;
    backend
        .action_repo()
        .save(&action)
        .await
        .expect("save action");
    id
}

fn make_trigger(action_id: ActionId, kind: TriggerKind) -> Trigger {
    Trigger {
        id: TriggerId::new(),
        action_id,
        kind,
        config: BTreeMap::new(),
    }
}

#[tokio::test]
async fn save_then_list_for_action_roundtrips() {
    let backend = setup().await;
    let action_id = insert_action(&backend).await;
    let trigger = make_trigger(action_id, TriggerKind::TwitchCheer);
    let id = trigger.id;
    backend.trigger_repo().save(&trigger).await.expect("save");
    let triggers = backend
        .trigger_repo()
        .list_for_action(action_id)
        .await
        .expect("list");
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].id, id);
    assert_eq!(triggers[0].action_id, action_id);
}

#[tokio::test]
async fn list_for_action_returns_empty_for_missing_action() {
    let backend = setup().await;
    let triggers = backend
        .trigger_repo()
        .list_for_action(ActionId::new())
        .await
        .expect("list");
    assert!(triggers.is_empty());
}

#[tokio::test]
async fn delete_existing_trigger_returns_true() {
    let backend = setup().await;
    let action_id = insert_action(&backend).await;
    let trigger = make_trigger(action_id, TriggerKind::TwitchSubscribe);
    let id = trigger.id;
    backend.trigger_repo().save(&trigger).await.expect("save");
    assert!(backend.trigger_repo().delete(id).await.expect("delete"));
    let triggers = backend
        .trigger_repo()
        .list_for_action(action_id)
        .await
        .expect("list");
    assert!(triggers.is_empty());
}

#[tokio::test]
async fn delete_missing_trigger_returns_false() {
    let backend = setup().await;
    assert!(
        !backend
            .trigger_repo()
            .delete(TriggerId::new())
            .await
            .expect("delete")
    );
}

#[tokio::test]
async fn list_for_action_scoped_to_action() {
    let backend = setup().await;
    let action_a = insert_action(&backend).await;
    let action_b = insert_action(&backend).await;

    backend
        .trigger_repo()
        .save(&make_trigger(action_a, TriggerKind::TwitchCheer))
        .await
        .expect("save a1");
    backend
        .trigger_repo()
        .save(&make_trigger(action_a, TriggerKind::TwitchRaid))
        .await
        .expect("save a2");
    backend
        .trigger_repo()
        .save(&make_trigger(action_b, TriggerKind::TwitchSubscribe))
        .await
        .expect("save b1");

    let for_a = backend
        .trigger_repo()
        .list_for_action(action_a)
        .await
        .expect("list a");
    assert_eq!(for_a.len(), 2);
    assert!(for_a.iter().all(|t| t.action_id == action_a));

    let for_b = backend
        .trigger_repo()
        .list_for_action(action_b)
        .await
        .expect("list b");
    assert_eq!(for_b.len(), 1);
}

#[tokio::test]
async fn trigger_config_survives_roundtrip() {
    use forge_types::Variant;

    let backend = setup().await;
    let action_id = insert_action(&backend).await;
    let mut config = BTreeMap::new();
    config.insert("min_bits".to_owned(), Variant::Int(100));
    let trigger = Trigger {
        id: TriggerId::new(),
        action_id,
        kind: TriggerKind::TwitchCheer,
        config,
    };
    backend.trigger_repo().save(&trigger).await.expect("save");
    let triggers = backend
        .trigger_repo()
        .list_for_action(action_id)
        .await
        .expect("list");
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].config.get("min_bits"), Some(&Variant::Int(100)));
}
