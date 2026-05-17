#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Action, ActionId, QueueId, SubActionSpec};

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup() -> SqliteBackend {
    SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open")
}

async fn insert_default_queue(backend: &SqliteBackend) -> QueueId {
    let q = backend
        .queue_repo()
        .get_by_name("Default")
        .await
        .expect("get default queue");
    q.expect("default queue must exist after migration").id
}

fn make_action(name: &str, queue_id: QueueId) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        description: None,
        sub_actions: vec![],
    }
}

#[tokio::test]
async fn save_then_get_roundtrips_action() {
    let backend = setup().await;
    let queue_id = insert_default_queue(&backend).await;
    let action = make_action("greet", queue_id);
    let id = action.id;
    backend.action_repo().save(&action).await.expect("save");
    let got = backend.action_repo().get(id).await.expect("get");
    assert!(got.is_some());
    let got = got.unwrap();
    assert_eq!(got.id, id);
    assert_eq!(got.name, "greet");
    assert!(got.enabled);
}

#[tokio::test]
async fn get_missing_action_returns_none() {
    let backend = setup().await;
    let got = backend
        .action_repo()
        .get(ActionId::new())
        .await
        .expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn save_updates_existing_action() {
    let backend = setup().await;
    let queue_id = insert_default_queue(&backend).await;
    let mut action = make_action("updatable", queue_id);
    let id = action.id;
    backend.action_repo().save(&action).await.expect("save 1");

    action.name = "updated".to_owned();
    backend.action_repo().save(&action).await.expect("save 2");

    let got = backend.action_repo().get(id).await.expect("get").unwrap();
    assert_eq!(got.name, "updated");
}

#[tokio::test]
async fn delete_existing_action_returns_true() {
    let backend = setup().await;
    let queue_id = insert_default_queue(&backend).await;
    let action = make_action("to_delete", queue_id);
    let id = action.id;
    backend.action_repo().save(&action).await.expect("save");
    assert!(backend.action_repo().delete(id).await.expect("delete"));
    assert!(backend.action_repo().get(id).await.expect("get").is_none());
}

#[tokio::test]
async fn delete_missing_action_returns_false() {
    let backend = setup().await;
    assert!(
        !backend
            .action_repo()
            .delete(ActionId::new())
            .await
            .expect("delete")
    );
}

#[tokio::test]
async fn list_returns_all_actions() {
    let backend = setup().await;
    let queue_id = insert_default_queue(&backend).await;
    backend
        .action_repo()
        .save(&make_action("alpha", queue_id))
        .await
        .expect("save a");
    backend
        .action_repo()
        .save(&make_action("beta", queue_id))
        .await
        .expect("save b");
    let all = backend.action_repo().list().await.expect("list");
    assert_eq!(all.len(), 2);
    assert!(all.iter().any(|a| a.name == "alpha"));
    assert!(all.iter().any(|a| a.name == "beta"));
}

#[tokio::test]
async fn list_by_group_scopes_to_group() {
    let backend = setup().await;
    let queue_id = insert_default_queue(&backend).await;
    let mut in_group = make_action("chat_greet", queue_id);
    in_group.group = Some("Chat".to_owned());
    backend
        .action_repo()
        .save(&in_group)
        .await
        .expect("save grouped");
    backend
        .action_repo()
        .save(&make_action("ungrouped", queue_id))
        .await
        .expect("save ungrouped");

    let chat = backend
        .action_repo()
        .list_by_group(Some("Chat"))
        .await
        .expect("list_by_group");
    assert_eq!(chat.len(), 1);
    assert_eq!(chat[0].name, "chat_greet");

    let no_group = backend
        .action_repo()
        .list_by_group(None)
        .await
        .expect("list_by_group None");
    assert_eq!(no_group.len(), 1);
    assert_eq!(no_group[0].name, "ungrouped");
}

#[tokio::test]
async fn sub_actions_json_survives_roundtrip() {
    let backend = setup().await;
    let queue_id = insert_default_queue(&backend).await;
    let mut action = make_action("with_sub", queue_id);
    action.sub_actions = vec![SubActionSpec::Log {
        level: forge_types::LogLevel::Info,
        message: "hello".to_owned(),
    }];
    let id = action.id;
    backend.action_repo().save(&action).await.expect("save");
    let got = backend.action_repo().get(id).await.expect("get").unwrap();
    assert_eq!(got.sub_actions.len(), 1);
}

#[tokio::test]
async fn cascade_delete_removes_triggers_on_action_delete() {
    use forge_types::{Trigger, TriggerId, TriggerKind};
    use std::collections::BTreeMap;

    let backend = setup().await;
    let queue_id = insert_default_queue(&backend).await;
    let action = make_action("cascade_action", queue_id);
    let action_id = action.id;
    backend
        .action_repo()
        .save(&action)
        .await
        .expect("save action");

    let trigger = Trigger {
        id: TriggerId::new(),
        action_id,
        kind: TriggerKind::TwitchCheer,
        config: BTreeMap::new(),
    };
    backend
        .trigger_repo()
        .save(&trigger)
        .await
        .expect("save trigger");

    backend
        .action_repo()
        .delete(action_id)
        .await
        .expect("delete action");

    let triggers = backend
        .trigger_repo()
        .list_for_action(action_id)
        .await
        .expect("list_for_action");
    assert!(triggers.is_empty());
}
