#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Write→read round-trip of the action telemetry path. Rows are written through
//! the REAL `ActionRepo::record_execution` (which maps `ExecutionStatus` → the
//! stored status string) and read back through the REAL `telemetry()` query.
//! Driving both halves is deliberate: the swap-impl guard for the error path is
//! that `ExecutionStatus::Error` must map to the exact `'err'` the telemetry SQL
//! filters on - otherwise `errors_7d` would silently stay 0.

use forge_storage::{DataProvider, ExecutionStatus};
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

fn at(secs: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(secs).expect("valid unix timestamp")
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
async fn record_execution_success_reflects_in_telemetry() {
    let backend = setup().await;
    let queue_id = default_queue_id(&backend).await;
    let action = make_test_action("success_action", queue_id);
    let action_id = action.id;
    backend.action_repo().save(&action).await.expect("save");

    let midnight = OffsetDateTime::now_utc()
        .replace_time(time::Time::MIDNIGHT)
        .unix_timestamp();
    let started = at(midnight + 5);
    backend
        .action_repo()
        .record_execution(action_id, started, 250, ExecutionStatus::Success)
        .await
        .expect("record success");

    let telemetry = backend
        .action_repo()
        .telemetry(action_id)
        .await
        .expect("telemetry");

    assert_eq!(
        telemetry.runs_today, 1,
        "the success run counts toward today"
    );
    assert_eq!(
        telemetry.last_fired_at.map(|t| t.unix_timestamp()),
        Some(midnight + 5),
        "last_fired reflects the recorded started_at",
    );
    assert_eq!(
        telemetry.avg_duration_ms,
        Some(250),
        "avg reflects the recorded duration_ms",
    );
    assert_eq!(
        telemetry.errors_7d, 0,
        "a success must not count as an error"
    );
}

#[tokio::test]
async fn record_execution_error_is_counted_in_errors_7d() {
    // Swap-impl guard: this only passes if `ExecutionStatus::Error` maps to the
    // exact `'err'` string the telemetry SQL filters on. Any other mapping either
    // trips the `CHECK (status IN ('ok','err'))` constraint (record_execution
    // errors) or lands the row outside the `status = 'err'` filter (errors_7d = 0).
    let backend = setup().await;
    let queue_id = default_queue_id(&backend).await;
    let action = make_test_action("error_action", queue_id);
    let action_id = action.id;
    backend.action_repo().save(&action).await.expect("save");

    let started = OffsetDateTime::now_utc();
    backend
        .action_repo()
        .record_execution(action_id, started, 50, ExecutionStatus::Error)
        .await
        .expect("record error");

    let telemetry = backend
        .action_repo()
        .telemetry(action_id)
        .await
        .expect("telemetry");

    assert_eq!(
        telemetry.errors_7d, 1,
        "the error run is counted in errors_7d"
    );
    assert_eq!(telemetry.runs_today, 1, "an error is still a run of today");
}

#[tokio::test]
async fn telemetry_aggregates_multiple_recorded_executions() {
    let backend = setup().await;
    let queue_id = default_queue_id(&backend).await;
    let action = make_test_action("stats_action", queue_id);
    let action_id = action.id;
    backend.action_repo().save(&action).await.expect("save");

    let now = OffsetDateTime::now_utc();
    let midnight = now.replace_time(time::Time::MIDNIGHT).unix_timestamp();

    let rows = [
        (at(midnight + 10), 100, ExecutionStatus::Success), // today
        (at(midnight + 20), 200, ExecutionStatus::Success), // today
        (at(midnight + 30), 150, ExecutionStatus::Error),   // today, err
        (now - time::Duration::days(3), 300, ExecutionStatus::Error), // within 7d, err
        (now - time::Duration::days(8), 400, ExecutionStatus::Error), // outside 7d, err
    ];
    for (started, dur, status) in rows {
        backend
            .action_repo()
            .record_execution(action_id, started, dur, status)
            .await
            .expect("record");
    }

    let telemetry = backend
        .action_repo()
        .telemetry(action_id)
        .await
        .expect("telemetry");

    assert_eq!(
        telemetry.runs_today, 3,
        "only the three midnight+N rows are today"
    );
    // 7-day window boundary: today err + 3-day err count; the 8-day err is excluded.
    assert_eq!(telemetry.errors_7d, 2, "errors_7d honors the 7-day window");
    // avg has no time window: (100+200+150+300+400)/5 = 230.
    assert_eq!(
        telemetry.avg_duration_ms,
        Some(230),
        "avg over every recorded row"
    );
    assert_eq!(
        telemetry.last_fired_at.map(|t| t.unix_timestamp()),
        Some(midnight + 30),
        "last_fired is the max started_at",
    );
}

#[tokio::test]
async fn prune_executions_before_removes_older_rows_and_telemetry_excludes_them() {
    let backend = setup().await;
    let queue_id = default_queue_id(&backend).await;
    let action = make_test_action("prune_action", queue_id);
    let action_id = action.id;
    backend.action_repo().save(&action).await.expect("save");

    let now = OffsetDateTime::now_utc();
    // 8 days old - beyond the execution-retention floor; its 999ms must vanish.
    backend
        .action_repo()
        .record_execution(
            action_id,
            now - time::Duration::days(8),
            999,
            ExecutionStatus::Success,
        )
        .await
        .expect("record old");
    let recent = at(now.unix_timestamp());
    backend
        .action_repo()
        .record_execution(action_id, recent, 100, ExecutionStatus::Success)
        .await
        .expect("record recent");

    let removed = backend
        .action_repo()
        .prune_executions_before(now - time::Duration::days(7))
        .await
        .expect("prune");

    assert_eq!(
        removed, 1,
        "only the 8-day-old execution predates the 7-day cutoff"
    );
    let telemetry = backend
        .action_repo()
        .telemetry(action_id)
        .await
        .expect("telemetry");
    assert_eq!(
        telemetry.avg_duration_ms,
        Some(100),
        "the pruned row's 999ms duration no longer contributes to the average",
    );
    assert_eq!(
        telemetry.last_fired_at.map(|t| t.unix_timestamp()),
        Some(recent.unix_timestamp()),
        "last_fired reflects the surviving execution",
    );
}

#[tokio::test]
async fn prune_executions_before_uses_second_precision_at_cutoff_boundary() {
    // Unit guard: action_executions.started_at is stored in SECONDS. A row one
    // second below the cutoff must be pruned; the row exactly at the cutoff and
    // one second above must survive the strict `<` comparison. A seconds/millis
    // (1000x) unit error would misclassify every row and fail this.
    let backend = setup().await;
    let queue_id = default_queue_id(&backend).await;
    let action = make_test_action("boundary_action", queue_id);
    let action_id = action.id;
    backend.action_repo().save(&action).await.expect("save");

    let base = OffsetDateTime::now_utc().unix_timestamp();
    backend
        .action_repo()
        .record_execution(action_id, at(base - 1), 1, ExecutionStatus::Success)
        .await
        .expect("record before");
    backend
        .action_repo()
        .record_execution(action_id, at(base), 100, ExecutionStatus::Success)
        .await
        .expect("record at cutoff");
    backend
        .action_repo()
        .record_execution(action_id, at(base + 1), 200, ExecutionStatus::Success)
        .await
        .expect("record after");

    let removed = backend
        .action_repo()
        .prune_executions_before(at(base))
        .await
        .expect("prune");

    assert_eq!(
        removed, 1,
        "only the execution one second before the cutoff is pruned"
    );
    let telemetry = backend
        .action_repo()
        .telemetry(action_id)
        .await
        .expect("telemetry");
    assert_eq!(
        telemetry.avg_duration_ms,
        Some(150),
        "average is over the at-cutoff (100) and after-cutoff (200) survivors only",
    );
    assert_eq!(
        telemetry.last_fired_at.map(|t| t.unix_timestamp()),
        Some(base + 1),
        "the cutoff+1s row survives the strict `<` prune",
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

    let midnight = OffsetDateTime::now_utc()
        .replace_time(time::Time::MIDNIGHT)
        .unix_timestamp();

    backend
        .action_repo()
        .record_execution(action_a.id, at(midnight + 10), 50, ExecutionStatus::Success)
        .await
        .expect("record a");
    backend
        .action_repo()
        .record_execution(action_b.id, at(midnight + 20), 500, ExecutionStatus::Error)
        .await
        .expect("record b");

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

    // Action A's error count must not leak B's error, and vice versa.
    assert_eq!(tel_a.errors_7d, 0, "a's error count excludes b's error row");
    assert_eq!(
        tel_a.avg_duration_ms,
        Some(50),
        "a's avg excludes b's duration"
    );
    assert_eq!(tel_b.errors_7d, 1, "b's error row is scoped to b");
    assert_eq!(
        tel_b.avg_duration_ms,
        Some(500),
        "b's avg excludes a's duration"
    );
}
