#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::{ActionRecord, ActionRepo, CommandRecord, CommandRepo};
use forge_storage_sqlite::{SqliteActionRepo, SqliteCommandRepo, apply_migrations};
use forge_types::{ActionId, CommandId};

async fn setup() -> (SqliteActionRepo, SqliteCommandRepo) {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");
    (
        SqliteActionRepo::new(pool.clone()),
        SqliteCommandRepo::new(pool),
    )
}

async fn insert_action(action_repo: &SqliteActionRepo) -> ActionId {
    let id = ActionId::new();
    action_repo
        .upsert(ActionRecord {
            id,
            name: format!("action_{id}"),
            config_json: "{}".to_owned(),
            created_at: time::OffsetDateTime::now_utc(),
            last_modified: time::OffsetDateTime::now_utc(),
        })
        .await
        .expect("insert action");
    id
}

fn make_command(name: &str, action_id: ActionId) -> CommandRecord {
    CommandRecord {
        id: CommandId::new(),
        name: name.to_owned(),
        action_id,
        cooldown_ms: 5000,
        permission: "viewer".to_owned(),
        enabled: true,
        created_at: time::OffsetDateTime::now_utc(),
        last_modified: time::OffsetDateTime::now_utc(),
    }
}

#[tokio::test]
async fn upsert_then_get_roundtrips_command() {
    let (action_repo, command_repo) = setup().await;
    let action_id = insert_action(&action_repo).await;
    let command = make_command("!hello", action_id);
    let id = command.id;
    command_repo.upsert(command).await.expect("upsert");
    let got = command_repo.get(id).await.expect("get");
    assert!(got.is_some());
    let got = got.unwrap();
    assert_eq!(got.id, id);
    assert_eq!(got.name, "!hello");
    assert_eq!(got.cooldown_ms, 5000);
    assert_eq!(got.permission, "viewer");
    assert!(got.enabled);
}

#[tokio::test]
async fn get_missing_command_returns_none() {
    let (_, command_repo) = setup().await;
    let got = command_repo.get(CommandId::new()).await.expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn get_by_name_finds_existing_command() {
    let (action_repo, command_repo) = setup().await;
    let action_id = insert_action(&action_repo).await;
    command_repo
        .upsert(make_command("!shoutout", action_id))
        .await
        .expect("upsert");
    let got = command_repo
        .get_by_name("!shoutout")
        .await
        .expect("get_by_name");
    assert!(got.is_some());
    assert_eq!(got.unwrap().name, "!shoutout");
}

#[tokio::test]
async fn get_by_name_missing_returns_none() {
    let (_, command_repo) = setup().await;
    let got = command_repo
        .get_by_name("!ghost")
        .await
        .expect("get_by_name");
    assert!(got.is_none());
}

#[tokio::test]
async fn delete_existing_command_returns_true() {
    let (action_repo, command_repo) = setup().await;
    let action_id = insert_action(&action_repo).await;
    let command = make_command("!del", action_id);
    let id = command.id;
    command_repo.upsert(command).await.expect("upsert");
    assert!(command_repo.delete(id).await.expect("delete"));
    assert!(command_repo.get(id).await.expect("get").is_none());
}

#[tokio::test]
async fn delete_missing_command_returns_false() {
    let (_, command_repo) = setup().await;
    assert!(!command_repo.delete(CommandId::new()).await.expect("delete"));
}

#[tokio::test]
async fn list_for_action_returns_only_that_actions_commands() {
    let (action_repo, command_repo) = setup().await;
    let action_a = insert_action(&action_repo).await;
    let action_b = insert_action(&action_repo).await;
    command_repo
        .upsert(make_command("!a1", action_a))
        .await
        .expect("upsert a1");
    command_repo
        .upsert(make_command("!a2", action_a))
        .await
        .expect("upsert a2");
    command_repo
        .upsert(make_command("!b1", action_b))
        .await
        .expect("upsert b1");

    let for_a = command_repo
        .list_for_action(action_a)
        .await
        .expect("list_for_action");
    assert_eq!(for_a.len(), 2);
    assert!(for_a.iter().all(|c| c.action_id == action_a));
}

#[tokio::test]
async fn list_returns_all_commands() {
    let (action_repo, command_repo) = setup().await;
    let action_id = insert_action(&action_repo).await;
    command_repo
        .upsert(make_command("!first", action_id))
        .await
        .expect("upsert 1");
    command_repo
        .upsert(make_command("!second", action_id))
        .await
        .expect("upsert 2");
    let all = command_repo.list().await.expect("list");
    assert_eq!(all.len(), 2);
}
