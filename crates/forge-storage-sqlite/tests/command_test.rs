#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Action, ActionId, Command, CommandId, CommandPermission};

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
        execution_mode: forge_types::ExecutionMode::Sequential,
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

fn make_command(name: &str, action_id: ActionId) -> Command {
    Command {
        id: CommandId::new(),
        action_id,
        name: name.to_owned(),
        cooldown_secs: 30,
        permission: CommandPermission::Everyone,
    }
}

#[tokio::test]
async fn save_then_get_by_name_roundtrips() {
    let backend = setup().await;
    let action_id = insert_action(&backend).await;
    let command = make_command("!hello", action_id);
    backend.command_repo().save(&command).await.expect("save");
    let got = backend
        .command_repo()
        .get_by_name("!hello")
        .await
        .expect("get_by_name");
    assert!(got.is_some());
    let got = got.unwrap();
    assert_eq!(got.name, "!hello");
    assert_eq!(got.cooldown_secs, 30);
    assert_eq!(got.permission, CommandPermission::Everyone);
}

#[tokio::test]
async fn get_by_name_missing_returns_none() {
    let backend = setup().await;
    let got = backend
        .command_repo()
        .get_by_name("!ghost")
        .await
        .expect("get_by_name");
    assert!(got.is_none());
}

#[tokio::test]
async fn delete_existing_command_returns_true() {
    let backend = setup().await;
    let action_id = insert_action(&backend).await;
    let command = make_command("!del", action_id);
    let id = command.id;
    backend.command_repo().save(&command).await.expect("save");
    assert!(backend.command_repo().delete(id).await.expect("delete"));
    assert!(
        backend
            .command_repo()
            .get_by_name("!del")
            .await
            .expect("get")
            .is_none()
    );
}

#[tokio::test]
async fn delete_missing_command_returns_false() {
    let backend = setup().await;
    assert!(
        !backend
            .command_repo()
            .delete(CommandId::new())
            .await
            .expect("delete")
    );
}

#[tokio::test]
async fn list_returns_all_commands() {
    let backend = setup().await;
    let action_id = insert_action(&backend).await;
    backend
        .command_repo()
        .save(&make_command("!first", action_id))
        .await
        .expect("save 1");
    backend
        .command_repo()
        .save(&make_command("!second", action_id))
        .await
        .expect("save 2");
    let all = backend.command_repo().list().await.expect("list");
    assert_eq!(all.len(), 2);
    assert!(all.iter().any(|c| c.name == "!first"));
    assert!(all.iter().any(|c| c.name == "!second"));
}

#[tokio::test]
async fn save_updates_existing_command() {
    let backend = setup().await;
    let action_id = insert_action(&backend).await;
    let mut command = make_command("!updatable", action_id);
    backend.command_repo().save(&command).await.expect("save 1");

    command.cooldown_secs = 60;
    command.permission = CommandPermission::Moderator;
    backend.command_repo().save(&command).await.expect("save 2");

    let got = backend
        .command_repo()
        .get_by_name("!updatable")
        .await
        .expect("get")
        .unwrap();
    assert_eq!(got.cooldown_secs, 60);
    assert_eq!(got.permission, CommandPermission::Moderator);
}

#[tokio::test]
async fn cascade_delete_removes_commands_on_action_delete() {
    let backend = setup().await;
    let action_id = insert_action(&backend).await;
    backend
        .command_repo()
        .save(&make_command("!cascade", action_id))
        .await
        .expect("save command");

    backend
        .action_repo()
        .delete(action_id)
        .await
        .expect("delete action");

    let all = backend.command_repo().list().await.expect("list");
    assert!(all.is_empty());
}
