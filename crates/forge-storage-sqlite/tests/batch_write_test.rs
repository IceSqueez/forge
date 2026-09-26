#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_storage::{
    ChatHistoryRepo, EventLogRepo, HistoryRepo, ViewerMessage, ViewerPlatform, ViewerRepo,
};
use forge_storage_sqlite::{
    SqliteChatHistoryRepo, SqliteEventLogRepo, SqliteHistoryRepo, SqliteViewerRepo,
    apply_migrations, connect,
};
use forge_types::{
    ActionId, ChatSource, EventId, ExecutionContext, ExecutionMetadata, ExecutionOutcome,
    ModerationMarks, UnifiedChatRow,
};
use sqlx::SqlitePool;
use time::OffsetDateTime;

/// One more than a single multi-row statement carries, so a batch spans two statements.
const TWO_STATEMENTS: usize = 257;
const POISON: &str = "poison";

async fn pool() -> SqlitePool {
    let pool = connect("sqlite::memory:").await.unwrap();
    apply_migrations(&pool).await.unwrap();
    pool
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Makes SQLite abort the statement that inserts the row whose `column` equals `value`.
async fn poison(pool: &SqlitePool, table: &str, column: &str, value: &str) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER poison_{table} BEFORE INSERT ON {table}
         WHEN NEW.{column} = '{value}'
         BEGIN SELECT RAISE(ABORT, 'poisoned row'); END"
    )))
    .execute(pool)
    .await
    .unwrap();
}

fn events(n: usize) -> Vec<Arc<Event>> {
    (0..n)
        .map(|_| {
            Arc::new(Event::new(
                EventSource::Core,
                "batch.row",
                serde_json::Value::Null,
            ))
        })
        .collect()
}

fn chat_row(id: &str, author: &str) -> UnifiedChatRow {
    UnifiedChatRow {
        id: id.to_string(),
        event_id: EventId::new(),
        source: ChatSource::Twitch,
        received_at: OffsetDateTime::now_utc(),
        author: author.to_string(),
        author_color: None,
        body_segments: vec![],
        badges: vec![],
        is_event: false,
        event_detail: None,
        moderation: ModerationMarks::default(),
    }
}

fn run(action_id: ActionId) -> ExecutionContext {
    ExecutionContext {
        action_id,
        metadata: ExecutionMetadata::QuickAction {
            builtin_id: "obs".to_string(),
            label: "run".to_string(),
        },
        arg_stack_snapshot: BTreeMap::new(),
        started_at: OffsetDateTime::now_utc(),
        completed_at: None,
        telemetry: Vec::new(),
        outcome: ExecutionOutcome::Success,
    }
}

fn message(viewer_id: &str, username: &str) -> ViewerMessage {
    ViewerMessage {
        platform: ViewerPlatform::Twitch,
        viewer_id: viewer_id.to_string(),
        username: username.to_string(),
    }
}

#[tokio::test]
async fn an_event_batch_stores_every_row_across_statement_boundaries() {
    for rows in [
        1,
        TWO_STATEMENTS - 2,
        TWO_STATEMENTS - 1,
        TWO_STATEMENTS,
        2_049,
    ] {
        let pool = pool().await;
        let repo = SqliteEventLogRepo::new(pool.clone());

        repo.insert_batch(&events(rows)).await.unwrap();

        assert_eq!(count(&pool, "event_log").await, rows as i64, "{rows} rows");
    }
}

#[tokio::test]
async fn an_event_batch_with_one_failing_row_stores_nothing() {
    let pool = pool().await;
    poison(&pool, "event_log", "kind", POISON).await;
    let mut batch = events(TWO_STATEMENTS);
    batch.push(Arc::new(Event::new(
        EventSource::Core,
        POISON,
        serde_json::Value::Null,
    )));

    let result = SqliteEventLogRepo::new(pool.clone())
        .insert_batch(&batch)
        .await;

    assert_eq!(
        (result.is_err(), count(&pool, "event_log").await),
        (true, 0)
    );
}

#[tokio::test]
async fn a_chat_batch_with_one_failing_row_stores_nothing() {
    let pool = pool().await;
    poison(&pool, "chat_history", "id", POISON).await;
    let mut batch: Vec<UnifiedChatRow> = (0..TWO_STATEMENTS)
        .map(|i| chat_row(&format!("m{i}"), "bob"))
        .collect();
    batch.push(chat_row(POISON, "bob"));

    let result = SqliteChatHistoryRepo::new(pool.clone())
        .append_batch(&batch)
        .await;

    assert_eq!(
        (result.is_err(), count(&pool, "chat_history").await),
        (true, 0)
    );
}

#[tokio::test]
async fn a_run_history_batch_with_one_failing_run_stores_nothing() {
    let pool = pool().await;
    let failing = ActionId::new();
    poison(&pool, "action_history", "action_id", &failing.to_string()).await;
    let mut batch: Vec<ExecutionContext> =
        (0..TWO_STATEMENTS).map(|_| run(ActionId::new())).collect();
    batch.push(run(failing));

    let result = SqliteHistoryRepo::new(pool.clone())
        .save_batch(&batch)
        .await;

    assert_eq!(
        (result.is_err(), count(&pool, "action_history").await),
        (true, 0)
    );
}

#[tokio::test]
async fn a_viewer_batch_with_one_failing_row_stores_nothing() {
    let pool = pool().await;
    poison(&pool, "viewers", "viewer_id", POISON).await;
    let mut batch: Vec<ViewerMessage> = (0..TWO_STATEMENTS)
        .map(|i| message(&format!("v{i}"), "name"))
        .collect();
    batch.push(message(POISON, "name"));

    let result = SqliteViewerRepo::new(pool.clone())
        .record_messages(&batch)
        .await;

    assert_eq!((result.is_err(), count(&pool, "viewers").await), (true, 0));
}

#[tokio::test]
async fn an_event_already_stored_keeps_its_row_and_the_rest_of_the_batch_lands() {
    let pool = pool().await;
    let repo = SqliteEventLogRepo::new(pool.clone());
    let mut stored = Event::new(EventSource::Core, "original", serde_json::Value::Null);
    stored.timestamp = OffsetDateTime::from_unix_timestamp(1_000_000).unwrap();
    repo.insert(&stored).await.unwrap();
    let mut duplicate = stored.clone();
    duplicate.kind = "overwrite".to_string();
    let fresh = Event::new(EventSource::Core, "fresh", serde_json::Value::Null);

    repo.insert_batch(&[Arc::new(duplicate), Arc::new(fresh.clone())])
        .await
        .unwrap();

    let kinds = (
        repo.get(stored.id).await.unwrap().map(|event| event.kind),
        repo.get(fresh.id).await.unwrap().map(|event| event.kind),
    );
    assert_eq!(
        kinds,
        (Some("original".to_string()), Some("fresh".to_string()))
    );
}

#[tokio::test]
async fn a_chat_message_already_stored_keeps_its_row_and_the_rest_of_the_batch_lands() {
    let pool = pool().await;
    let repo = SqliteChatHistoryRepo::new(pool.clone());
    repo.append(&chat_row("m1", "original")).await.unwrap();

    repo.append_batch(&[chat_row("m1", "overwrite"), chat_row("m2", "fresh")])
        .await
        .unwrap();

    let mut authors: Vec<(String, String)> = repo
        .list_recent(10)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.id, row.author))
        .collect();
    authors.sort();
    assert_eq!(
        authors,
        vec![
            ("m1".to_string(), "original".to_string()),
            ("m2".to_string(), "fresh".to_string()),
        ]
    );
}

#[tokio::test]
async fn a_viewer_batch_counts_every_message_and_keeps_the_latest_name() {
    let pool = pool().await;
    let repo = SqliteViewerRepo::new(pool.clone());
    repo.record_message(ViewerPlatform::Twitch, "v1", "old-name")
        .await
        .unwrap();
    let first_seen = repo
        .get(ViewerPlatform::Twitch, "v1")
        .await
        .unwrap()
        .unwrap()
        .first_seen_at;

    repo.record_messages(&[
        message("v1", "mid-name"),
        message("v2", "other"),
        message("v1", "new-name"),
    ])
    .await
    .unwrap();

    let v1 = repo
        .get(ViewerPlatform::Twitch, "v1")
        .await
        .unwrap()
        .unwrap();
    let v2 = repo
        .get(ViewerPlatform::Twitch, "v2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            v1.message_count,
            v1.username.as_str(),
            v1.first_seen_at,
            v2.message_count
        ),
        (3, "new-name", first_seen, 1)
    );
}
