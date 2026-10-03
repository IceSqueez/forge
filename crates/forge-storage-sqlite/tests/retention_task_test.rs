#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_events::{Event, EventSource};
use forge_storage::{
    DataProvider, MissedRunPolicy, ScheduledRunOutcome, ScheduledRunSpec,
    set_event_log_retention_days,
};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{ActionId, EventId};
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use time::OffsetDateTime;

const TEST_KEY: [u8; 32] = [0xab; 32];
const SLOW_CADENCE: Duration = Duration::from_secs(2);
const FAST_CADENCE: Duration = Duration::from_millis(50);
const CHUNK_ROWS: usize = 1_000;
const POLL: Duration = Duration::from_millis(5);

struct Db {
    backend: SqliteBackend,
    side: SqlitePool,
    opened: Instant,
    _dir: tempfile::TempDir,
}

async fn open(cadence: Duration) -> Db {
    let dir = tempfile::tempdir().expect("create tmpdir");
    let url = format!("sqlite:{}", dir.path().join("test.db").display());
    let opened = Instant::now();
    let backend =
        SqliteBackend::open_for_test(&url, TEST_KEY, dir.path().join("media"), Some(cadence))
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
        opened,
        _dir: dir,
    }
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
    })
}

fn aged_batch(n: usize, age: time::Duration) -> Vec<Arc<Event>> {
    (0..n).map(|_| aged(age)).collect()
}

impl Db {
    async fn store(&self, events: &[Arc<Event>]) {
        self.backend
            .event_log_repo()
            .insert_batch(events)
            .await
            .expect("insert");
    }

    async fn rows(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM event_log")
            .fetch_one(&self.side)
            .await
            .unwrap()
    }

    async fn contains(&self, id: EventId) -> bool {
        self.backend
            .event_log_repo()
            .get(id)
            .await
            .expect("get")
            .is_some()
    }

    async fn rows_reach(&self, deadline: Duration, done: impl Fn(i64) -> bool) -> bool {
        while self.opened.elapsed() < deadline {
            if done(self.rows().await) {
                return true;
            }
            tokio::time::sleep(POLL).await;
        }
        done(self.rows().await)
    }
}

#[tokio::test]
async fn one_sweep_deletes_every_row_older_than_the_window_across_whole_chunks_and_nothing_newer() {
    let db = open(SLOW_CADENCE).await;
    db.store(&aged_batch(2 * CHUNK_ROWS, time::Duration::days(8)))
        .await;
    let just_inside = aged(time::Duration::days(7) - time::Duration::hours(1));
    let recent = aged(time::Duration::days(1));
    db.store(&[Arc::clone(&just_inside), Arc::clone(&recent)])
        .await;

    let deadline = SLOW_CADENCE + SLOW_CADENCE / 2;
    let swept_once = db.rows_reach(deadline, |rows| rows == 2).await;
    let over_deleted = db.rows_reach(deadline, |rows| rows < 2).await;

    assert_eq!(
        (
            swept_once,
            over_deleted,
            db.contains(just_inside.id).await,
            db.contains(recent.id).await
        ),
        (true, false, true, true)
    );
}

#[tokio::test]
async fn a_batched_write_lands_between_prune_chunks_while_older_rows_remain() {
    let db = open(FAST_CADENCE).await;
    let backlog = (3 * CHUNK_ROWS) as i64;
    db.store(&aged_batch(3 * CHUNK_ROWS, time::Duration::days(30)))
        .await;

    let mid_prune = db
        .rows_reach(SLOW_CADENCE, |rows| rows > 0 && rows < backlog)
        .await;
    db.store(&aged_batch(1, time::Duration::ZERO)).await;
    let old_rows_left = db.rows().await - 1;

    assert!(
        mid_prune && old_rows_left > 0,
        "the write must not wait for the whole backlog (mid-prune seen: {mid_prune}, old rows left: {old_rows_left})"
    );
}

#[tokio::test]
async fn shrinking_the_window_re_sweeps_at_once_instead_of_at_the_next_scheduled_sweep() {
    let db = open(SLOW_CADENCE).await;
    let expired = aged(time::Duration::days(30));
    let five_days = aged(time::Duration::days(5));
    db.store(&[Arc::clone(&expired), Arc::clone(&five_days)])
        .await;
    assert!(
        db.rows_reach(SLOW_CADENCE * 2, |rows| rows == 1).await,
        "the first sweep never ran"
    );

    set_event_log_retention_days(&db.backend, 3).await.unwrap();
    let changed_at = db.opened.elapsed();
    let re_swept = db
        .rows_reach(changed_at + SLOW_CADENCE / 2, |rows| rows == 0)
        .await;

    assert!(
        re_swept,
        "a 5-day-old row must go as soon as the window shrinks to 3 days"
    );
}

#[tokio::test]
async fn the_pruner_stops_once_its_backend_is_dropped() {
    let db = open(FAST_CADENCE).await;
    db.store(&[aged(time::Duration::days(30))]).await;
    assert!(
        db.rows_reach(SLOW_CADENCE, |rows| rows == 0).await,
        "the pruner never ran"
    );
    let Db {
        backend,
        side,
        _dir,
        ..
    } = db;

    drop(backend);
    sqlx::query("INSERT INTO event_log (id, source, kind, timestamp, payload, replay) VALUES ('orphan', 'Twitch', 'x', 0, 'null', 0)")
        .execute(&side)
        .await
        .unwrap();
    tokio::time::sleep(FAST_CADENCE * 5).await;
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_log")
        .fetch_one(&side)
        .await
        .unwrap();

    assert_eq!(rows, 1, "a dropped backend's pruner must not keep deleting");
}

async fn resolved_run(db: &Db, age: time::Duration) -> forge_storage::ScheduledRunId {
    let repo = db.backend.scheduled_run_repo();
    let resolved_at = OffsetDateTime::now_utc() - age;
    let id = repo
        .schedule(&ScheduledRunSpec {
            target_action_id: ActionId::new(),
            due_at: resolved_at,
            key: None,
            missed_run_policy: MissedRunPolicy::RunLateOnce,
            args: Default::default(),
            scheduled_by_action: None,
            scheduled_by_run: None,
            trigger_event_id: None,
            scheduled_at: resolved_at,
            label: "follow-up".to_owned(),
        })
        .await
        .unwrap()
        .id;
    repo.settle(id, ScheduledRunOutcome::Dispatched, None, resolved_at)
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn a_sweep_prunes_resolved_scheduled_runs_older_than_the_window_and_keeps_pending_ones() {
    let db = open(FAST_CADENCE).await;
    let repo = db.backend.scheduled_run_repo();
    let expired = resolved_run(&db, time::Duration::days(8)).await;
    let recent = resolved_run(&db, time::Duration::days(1)).await;
    let pending = repo
        .schedule(&ScheduledRunSpec {
            target_action_id: ActionId::new(),
            due_at: OffsetDateTime::now_utc() - time::Duration::days(30),
            key: None,
            missed_run_policy: MissedRunPolicy::RunLateOnce,
            args: Default::default(),
            scheduled_by_action: None,
            scheduled_by_run: None,
            trigger_event_id: None,
            scheduled_at: OffsetDateTime::now_utc() - time::Duration::days(31),
            label: "overdue".to_owned(),
        })
        .await
        .unwrap()
        .id;

    let deadline = Instant::now() + SLOW_CADENCE;
    while repo.get(expired).await.unwrap().is_some() && Instant::now() < deadline {
        tokio::time::sleep(POLL).await;
    }

    let mut present = Vec::new();
    for id in [expired, recent, pending] {
        present.push(repo.get(id).await.unwrap().is_some());
    }
    assert_eq!(present, vec![false, true, true]);
}
