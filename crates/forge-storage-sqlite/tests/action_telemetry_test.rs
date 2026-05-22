#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Action, ActionId, ExecutionMode, QueueId};
use time::OffsetDateTime;

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup() -> SqliteBackend {
    SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open")
}

async fn default_queue_id(backend: &SqliteBackend) -> QueueId {
    backend
        .queue_repo()
        .get_by_name("Default")
        .await
        .expect("get default queue")
        .expect("default queue seeded by migration")
        .id
}

fn make_test_action(name: &str, queue_id: QueueId) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![],
    }
}

#[tokio::test]
async fn telemetry_returns_defaults_for_action_with_no_executions() {
    let backend = setup().await;
    let queue_id = default_queue_id(&backend).await;
    let action = make_test_action("empty_action", queue_id);
    let action_id = action.id;
    backend.action_repo().save(&action).await.expect("save");

    let telemetry = backend
        .action_repo()
        .telemetry(action_id)
        .await
        .expect("telemetry");

    assert!(telemetry.last_fired_at.is_none());
    assert_eq!(telemetry.runs_today, 0);
    assert_eq!(telemetry.avg_duration_ms, None);
    assert_eq!(telemetry.errors_7d, 0);
}

#[tokio::test]
async fn telemetry_aggregates_executions_correctly() {
    let backend = setup().await;
    let queue_id = default_queue_id(&backend).await;
    let action = make_test_action("stats_action", queue_id);
    let action_id = action.id;
    backend.action_repo().save(&action).await.expect("save");

    let now = OffsetDateTime::now_utc();
    let midnight = now.replace_time(time::Time::MIDNIGHT).unix_timestamp();

    // 3 executions today: 2 ok, 1 err
    let today_1 = midnight + 1;
    let today_2 = midnight + 2;
    let today_3 = midnight + 3;
    // 2 executions within the past 7 days but before today: 1 ok, 1 err
    let week_1 = now.unix_timestamp() - 3 * 86400;
    let week_2 = now.unix_timestamp() - 5 * 86400;

    backend
        .insert_execution_for_test(action_id, today_1, 100, "ok")
        .await
        .expect("insert today_1");
    backend
        .insert_execution_for_test(action_id, today_2, 200, "ok")
        .await
        .expect("insert today_2");
    backend
        .insert_execution_for_test(action_id, today_3, 150, "err")
        .await
        .expect("insert today_3");
    backend
        .insert_execution_for_test(action_id, week_1, 300, "ok")
        .await
        .expect("insert week_1");
    backend
        .insert_execution_for_test(action_id, week_2, 250, "err")
        .await
        .expect("insert week_2");

    let telemetry = backend
        .action_repo()
        .telemetry(action_id)
        .await
        .expect("telemetry");

    assert_eq!(telemetry.runs_today, 3, "runs_today");
    // errors within 7d: today_3 (err) + week_2 (err) = 2
    assert_eq!(telemetry.errors_7d, 2, "errors_7d");
    // avg of 100+200+150+300+250 = 1000 / 5 = 200
    assert_eq!(telemetry.avg_duration_ms, Some(200), "avg_duration_ms");
    // last fired = max of all 5 started_at values = today_3 (midnight+3)
    let last = telemetry.last_fired_at.expect("last_fired_at is Some");
    assert_eq!(
        last.unix_timestamp(),
        today_3,
        "last_fired_at unix_timestamp"
    );
}

#[tokio::test]
async fn telemetry_is_scoped_per_action() {
    let backend = setup().await;
    let queue_id = default_queue_id(&backend).await;
    let action_a = make_test_action("action_a", queue_id);
    let action_b = make_test_action("action_b", queue_id);
    backend.action_repo().save(&action_a).await.expect("save a");
    backend.action_repo().save(&action_b).await.expect("save b");

    let now = OffsetDateTime::now_utc();
    let midnight = now.replace_time(time::Time::MIDNIGHT).unix_timestamp();

    backend
        .insert_execution_for_test(action_a.id, midnight + 10, 50, "ok")
        .await
        .expect("insert a");
    backend
        .insert_execution_for_test(action_b.id, midnight + 20, 500, "err")
        .await
        .expect("insert b");

    let tel_a = backend
        .action_repo()
        .telemetry(action_a.id)
        .await
        .expect("telemetry a");
    let tel_b = backend
        .action_repo()
        .telemetry(action_b.id)
        .await
        .expect("telemetry b");

    assert_eq!(tel_a.runs_today, 1);
    assert_eq!(tel_a.errors_7d, 0);
    assert_eq!(tel_a.avg_duration_ms, Some(50));

    assert_eq!(tel_b.runs_today, 1);
    assert_eq!(tel_b.errors_7d, 1);
    assert_eq!(tel_b.avg_duration_ms, Some(500));
}
