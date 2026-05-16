#![allow(clippy::expect_used, clippy::unwrap_used)]

use loom_events::EventSource;
use loom_storage::{ActionRecord, ActionRepo, TriggerRecord, TriggerRepo};
use loom_storage_sqlite::{SqliteActionRepo, SqliteTriggerRepo, apply_migrations};
use loom_types::{ActionId, TriggerId};

async fn setup() -> (SqliteActionRepo, SqliteTriggerRepo) {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");
    (
        SqliteActionRepo::new(pool.clone()),
        SqliteTriggerRepo::new(pool),
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

fn make_trigger(action_id: ActionId, source: EventSource, enabled: bool) -> TriggerRecord {
    TriggerRecord {
        id: TriggerId::new(),
        name: format!("trigger_{}", TriggerId::new()),
        source,
        pattern_json: r#"{"min_bits":0}"#.to_owned(),
        action_id,
        enabled,
        created_at: time::OffsetDateTime::now_utc(),
        last_modified: time::OffsetDateTime::now_utc(),
    }
}

#[tokio::test]
async fn upsert_then_get_roundtrips_trigger() {
    let (action_repo, trigger_repo) = setup().await;
    let action_id = insert_action(&action_repo).await;
    let trigger = make_trigger(action_id, EventSource::Twitch, true);
    let id = trigger.id;
    trigger_repo.upsert(trigger).await.expect("upsert");
    let got = trigger_repo.get(id).await.expect("get");
    assert!(got.is_some());
    let got = got.unwrap();
    assert_eq!(got.id, id);
    assert_eq!(got.source, EventSource::Twitch);
    assert!(got.enabled);
    assert_eq!(got.action_id, action_id);
}

#[tokio::test]
async fn get_missing_trigger_returns_none() {
    let (_, trigger_repo) = setup().await;
    let got = trigger_repo.get(TriggerId::new()).await.expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn delete_existing_trigger_returns_true() {
    let (action_repo, trigger_repo) = setup().await;
    let action_id = insert_action(&action_repo).await;
    let trigger = make_trigger(action_id, EventSource::Core, true);
    let id = trigger.id;
    trigger_repo.upsert(trigger).await.expect("upsert");
    assert!(trigger_repo.delete(id).await.expect("delete"));
    assert!(trigger_repo.get(id).await.expect("get").is_none());
}

#[tokio::test]
async fn delete_missing_trigger_returns_false() {
    let (_, trigger_repo) = setup().await;
    assert!(!trigger_repo.delete(TriggerId::new()).await.expect("delete"));
}

#[tokio::test]
async fn list_for_action_scoped_to_action() {
    let (action_repo, trigger_repo) = setup().await;
    let action_a = insert_action(&action_repo).await;
    let action_b = insert_action(&action_repo).await;
    trigger_repo
        .upsert(make_trigger(action_a, EventSource::Twitch, true))
        .await
        .expect("upsert a1");
    trigger_repo
        .upsert(make_trigger(action_a, EventSource::YouTube, true))
        .await
        .expect("upsert a2");
    trigger_repo
        .upsert(make_trigger(action_b, EventSource::Core, true))
        .await
        .expect("upsert b1");

    let for_a = trigger_repo
        .list_for_action(action_a)
        .await
        .expect("list_for_action a");
    assert_eq!(for_a.len(), 2);
    assert!(for_a.iter().all(|t| t.action_id == action_a));

    let for_b = trigger_repo
        .list_for_action(action_b)
        .await
        .expect("list_for_action b");
    assert_eq!(for_b.len(), 1);
}

#[tokio::test]
async fn list_enabled_by_source_filters_disabled() {
    let (action_repo, trigger_repo) = setup().await;
    let action_id = insert_action(&action_repo).await;
    trigger_repo
        .upsert(make_trigger(action_id, EventSource::Twitch, true))
        .await
        .expect("upsert enabled");
    trigger_repo
        .upsert(make_trigger(action_id, EventSource::Twitch, false))
        .await
        .expect("upsert disabled");

    let enabled = trigger_repo
        .list_enabled_by_source(EventSource::Twitch)
        .await
        .expect("list_enabled_by_source");
    assert_eq!(enabled.len(), 1);
    assert!(enabled[0].enabled);
}

#[tokio::test]
async fn list_enabled_by_source_excludes_other_sources() {
    let (action_repo, trigger_repo) = setup().await;
    let action_id = insert_action(&action_repo).await;
    trigger_repo
        .upsert(make_trigger(action_id, EventSource::Twitch, true))
        .await
        .expect("upsert twitch");
    trigger_repo
        .upsert(make_trigger(action_id, EventSource::YouTube, true))
        .await
        .expect("upsert youtube");

    let twitch_only = trigger_repo
        .list_enabled_by_source(EventSource::Twitch)
        .await
        .expect("list_enabled_by_source");
    assert_eq!(twitch_only.len(), 1);
    assert_eq!(twitch_only[0].source, EventSource::Twitch);
}
