#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_storage::{
    DataProvider, ExecutionStatus, MissedRunPolicy, ScheduledRunId, ScheduledRunOutcome,
    ScheduledRunSpec, set_action_history_retention_days, set_event_log_retention_days,
};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{ActionId, EventId, ExecutionContext, ExecutionMetadata, ExecutionOutcome};
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use time::OffsetDateTime;
use tokio::sync::Notify;

const TEST_KEY: [u8; 32] = [0xab; 32];
const CADENCE: Duration = Duration::from_millis(50);
const HANG_GUARD: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(5);
const CHUNK_ROWS: usize = 1_000;
const BEYOND_ANY_WINDOW: time::Duration = time::Duration::days(3_650);

struct Db {
    backend: SqliteBackend,
    side: SqlitePool,
    url: String,
    dir: tempfile::TempDir,
}

async fn open() -> Db {
    let dir = tempfile::tempdir().expect("create tmpdir");
    let url = format!("sqlite:{}", dir.path().join("test.db").display());
    open_at(dir, url).await
}

async fn open_at(dir: tempfile::TempDir, url: String) -> Db {
    let backend =
        SqliteBackend::open_for_test(&url, TEST_KEY, dir.path().join("media"), Some(CADENCE))
            .await
            .expect("open");
    let side = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("side connection");
    Db {
        backend,
        side,
        url,
        dir,
    }
}

async fn within_hang_guard<T>(work: impl Future<Output = T>) -> Option<T> {
    tokio::time::timeout(HANG_GUARD, work).await.ok()
}

fn aged(age: time::Duration) -> Arc<Event> {
    Arc::new(Event {
        id: EventId::new(),
        source: EventSource::Twitch,
        kind: "twitch.channel.chat.message".to_owned(),
        timestamp: OffsetDateTime::now_utc() - age,
        payload: serde_json::Value::Null,
        caused_by: None,
        replay: false,
        causation_depth: 0,
    })
}

fn aged_batch(n: usize, age: time::Duration) -> Vec<Arc<Event>> {
    (0..n).map(|_| aged(age)).collect()
}

fn spec(due_at: OffsetDateTime, scheduled_at: OffsetDateTime, label: &str) -> ScheduledRunSpec {
    ScheduledRunSpec {
        target_action_id: ActionId::new(),
        due_at,
        key: None,
        missed_run_policy: MissedRunPolicy::RunLateOnce,
        args: Default::default(),
        scheduled_by_action: None,
        scheduled_by_run: None,
        trigger_event_id: None,
        scheduled_at,
        label: label.to_owned(),
    }
}

impl Db {
    async fn restarted(self) -> Db {
        let Db {
            backend,
            side,
            url,
            dir,
        } = self;
        backend.shutdown().await;
        drop(backend);
        side.close().await;
        open_at(dir, url).await
    }

    async fn store(&self, events: &[Arc<Event>]) {
        self.backend
            .event_log_repo()
            .insert_batch(events)
            .await
            .expect("insert");
    }

    async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
            .fetch_one(&self.side)
            .await
            .unwrap()
    }

    async fn reaches(&self, table: &str, done: impl Fn(i64) -> bool) -> bool {
        within_hang_guard(async {
            while !done(self.count(table).await) {
                tokio::time::sleep(POLL).await;
            }
        })
        .await
        .is_some()
    }

    async fn resolved_run(&self, age: time::Duration) -> ScheduledRunId {
        let repo = self.backend.scheduled_run_repo();
        let resolved_at = OffsetDateTime::now_utc() - age;
        let id = repo
            .schedule(&spec(resolved_at, resolved_at, "follow-up"))
            .await
            .unwrap()
            .id;
        repo.settle(id, ScheduledRunOutcome::Dispatched, None, resolved_at)
            .await
            .unwrap();
        id
    }

    async fn is_scheduled(&self, id: ScheduledRunId) -> bool {
        self.backend
            .scheduled_run_repo()
            .get(id)
            .await
            .unwrap()
            .is_some()
    }

    async fn after_a_full_sweep(&self) {
        for _ in 0..2 {
            let marker = self.resolved_run(BEYOND_ANY_WINDOW).await;
            let pruned = within_hang_guard(async {
                while self.is_scheduled(marker).await {
                    tokio::time::sleep(POLL).await;
                }
            })
            .await;
            assert!(pruned.is_some(), "the pruner never completed a sweep");
        }
    }

    async fn record_run(&self, age: time::Duration) {
        let started_at = OffsetDateTime::now_utc() - age;
        let action_id = ActionId::new();
        self.backend
            .history_repo()
            .save(&ExecutionContext {
                action_id,
                metadata: ExecutionMetadata::Trigger {
                    event_id: EventId::new(),
                    trigger_kind: None,
                },
                arg_stack_snapshot: Default::default(),
                started_at,
                completed_at: Some(started_at),
                telemetry: vec![],
                outcome: ExecutionOutcome::Success,
            })
            .await
            .expect("save run");
        self.backend
            .action_repo()
            .record_execution(action_id, started_at, 5, ExecutionStatus::Success)
            .await
            .expect("record execution");
    }

    async fn contains(&self, id: EventId) -> bool {
        self.backend
            .event_log_repo()
            .get(id)
            .await
            .expect("get")
            .is_some()
    }
}

#[tokio::test]
async fn a_sweep_deletes_every_event_older_than_the_window_across_whole_chunks_and_nothing_newer() {
    let db = open().await;
    db.store(&aged_batch(2 * CHUNK_ROWS, time::Duration::days(8)))
        .await;
    let just_inside = aged(time::Duration::days(7) - time::Duration::hours(1));
    let recent = aged(time::Duration::days(1));
    db.store(&[Arc::clone(&just_inside), Arc::clone(&recent)])
        .await;

    db.after_a_full_sweep().await;

    assert_eq!(
        (
            db.count("event_log").await,
            db.contains(just_inside.id).await,
            db.contains(recent.id).await
        ),
        (2, true, true)
    );
}

#[tokio::test]
async fn shrinking_the_event_window_makes_the_running_pruner_apply_the_new_window() {
    let db = open().await;
    let five_days = aged(time::Duration::days(5));
    db.store(&[aged(time::Duration::days(30)), Arc::clone(&five_days)])
        .await;
    db.after_a_full_sweep().await;

    set_event_log_retention_days(&db.backend, 3).await.unwrap();

    assert!(
        db.reaches("event_log", |rows| rows == 0).await,
        "a 5-day-old event must go once the window shrinks to 3 days"
    );
}

struct PrunerStopWatch(Arc<Notify>);

struct StopMessage(bool);

impl tracing::field::Visit for StopMessage {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn Debug) {
        if field.name() == "message" && format!("{value:?}") == "retention pruner stopped" {
            self.0 = true;
        }
    }
}

impl tracing::Subscriber for PrunerStopWatch {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut message = StopMessage(false);
        event.record(&mut message);
        if message.0 {
            self.0.notify_one();
        }
    }

    fn enter(&self, _: &tracing::span::Id) {}

    fn exit(&self, _: &tracing::span::Id) {}
}

#[tokio::test]
async fn dropping_the_backend_stops_its_pruner() {
    let stopped = Arc::new(Notify::new());
    let _watch = tracing::subscriber::set_default(PrunerStopWatch(Arc::clone(&stopped)));
    let db = open().await;
    db.after_a_full_sweep().await;

    drop(db.backend);

    assert!(
        within_hang_guard(stopped.notified()).await.is_some(),
        "a dropped backend's pruner must stop"
    );
}

#[tokio::test]
async fn a_sweep_prunes_resolved_scheduled_runs_older_than_the_window_and_keeps_pending_ones() {
    let db = open().await;
    let expired = db.resolved_run(time::Duration::days(8)).await;
    let recent = db.resolved_run(time::Duration::days(1)).await;
    let now = OffsetDateTime::now_utc();
    let pending = db
        .backend
        .scheduled_run_repo()
        .schedule(&spec(
            now - time::Duration::days(30),
            now - time::Duration::days(31),
            "overdue",
        ))
        .await
        .unwrap()
        .id;

    db.after_a_full_sweep().await;

    let mut present = Vec::new();
    for id in [expired, recent, pending] {
        present.push(db.is_scheduled(id).await);
    }
    assert_eq!(present, vec![false, true, true]);
}

#[tokio::test]
async fn each_table_is_pruned_by_its_own_window_when_the_two_retentions_differ() {
    let db = open().await;
    set_event_log_retention_days(&db.backend, 3).await.unwrap();
    set_action_history_retention_days(&db.backend, 30)
        .await
        .unwrap();
    let db = db.restarted().await;
    db.store(&[aged(time::Duration::days(2)), aged(time::Duration::days(4))])
        .await;
    db.resolved_run(time::Duration::days(2)).await;
    db.resolved_run(time::Duration::days(4)).await;
    db.record_run(time::Duration::days(20)).await;
    db.record_run(time::Duration::days(40)).await;

    db.after_a_full_sweep().await;

    let mut survivors = Vec::new();
    for table in [
        "event_log",
        "scheduled_runs",
        "action_history",
        "action_executions",
    ] {
        survivors.push((table, db.count(table).await));
    }
    assert_eq!(
        survivors,
        vec![
            ("event_log", 1),
            ("scheduled_runs", 1),
            ("action_history", 1),
            ("action_executions", 1),
        ]
    );
}

#[tokio::test]
async fn shrinking_the_action_history_window_makes_the_running_pruner_apply_the_new_window() {
    let db = open().await;
    set_action_history_retention_days(&db.backend, 30)
        .await
        .unwrap();
    let db = db.restarted().await;
    db.record_run(time::Duration::days(40)).await;
    db.record_run(time::Duration::days(20)).await;
    db.after_a_full_sweep().await;

    set_action_history_retention_days(&db.backend, 10)
        .await
        .unwrap();

    assert!(
        db.reaches("action_history", |rows| rows == 0).await,
        "a 20-day-old run must go once the action history window shrinks to 10 days"
    );
}
