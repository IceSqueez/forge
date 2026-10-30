#![allow(clippy::expect_used)]
use std::collections::BTreeMap;

use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use forge_types::{
    ActionId, EventId, ExecutionContext, ExecutionMetadata, ExecutionOutcome, IntegrationId,
    SubActionOutcome, SubActionTelemetry, UserText,
};
use serde_json::Value;
use sqlx::SqlitePool;
use tempfile::TempDir;
use time::OffsetDateTime;

const VERSION_BEFORE_USER_TEXT: i64 = 59;
const USER_TEXT_REWRITE: &str = include_str!("../migrations/0060_action_history_user_text.sql");
const LEGACY_TEXT_VARIANTS: [&str; 2] = ["Failed", "Skipped"];

type Row = (i64, String, String);

fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(seconds).expect("a valid instant")
}

fn step(index: usize, outcome: SubActionOutcome) -> SubActionTelemetry {
    SubActionTelemetry {
        index,
        kind: format!("step.{index}"),
        started_at: at(1_700_000_000),
        duration_ms: 3,
        outcome,
        args_in: BTreeMap::new(),
        produced: BTreeMap::new(),
    }
}

fn run(outcome: ExecutionOutcome, telemetry: Vec<SubActionTelemetry>) -> ExecutionContext {
    ExecutionContext {
        action_id: ActionId::new(),
        metadata: ExecutionMetadata::Trigger {
            event_id: EventId::new(),
            trigger_kind: None,
        },
        arg_stack_snapshot: BTreeMap::new(),
        started_at: at(1_700_000_000),
        completed_at: Some(at(1_700_000_001)),
        telemetry,
        outcome,
    }
}

fn verbatim(text: &str) -> UserText {
    forge_types::VerbatimText::new(text).into()
}

fn failed_run_with_mixed_steps() -> ExecutionContext {
    run(
        ExecutionOutcome::Failed(verbatim("the run halted")),
        vec![
            step(0, SubActionOutcome::Success),
            step(1, SubActionOutcome::Failed(verbatim("boom"))),
            step(2, SubActionOutcome::Skipped(verbatim("condition false"))),
            step(
                3,
                SubActionOutcome::IntegrationDisabled(IntegrationId::new("twitch")),
            ),
            step(4, SubActionOutcome::Failed(verbatim("second failure"))),
        ],
    )
}

fn downgraded(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut map: serde_json::Map<String, Value> = map
                .into_iter()
                .map(|(key, inner)| (key, downgraded(inner)))
                .collect();
            for variant in LEGACY_TEXT_VARIANTS {
                if let Some(Value::Object(text)) = map.get(variant)
                    && let Some(Value::String(plain)) = text.get("verbatim")
                    && text.len() == 1
                {
                    let plain = plain.clone();
                    map.insert(variant.to_owned(), Value::String(plain));
                }
            }
            Value::Object(map)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(downgraded).collect()),
        other => other,
    }
}

fn legacy_json<T: serde::Serialize>(value: &T) -> String {
    downgraded(serde_json::to_value(value).expect("serializable")).to_string()
}

async fn schema_before_user_text() -> (SqlitePool, TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}", dir.path().join("forge.db").display());
    let pool = connect(&url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_USER_TEXT, &pool)
        .await
        .expect("the schema before coded user text");
    (pool, dir)
}

async fn insert_raw(pool: &SqlitePool, outcome: &str, context: &str) {
    sqlx::query(
        "INSERT INTO action_history
            (action_id, triggering_event_id, started_at, duration_ms, outcome, context)
         VALUES (?, NULL, 0, 0, ?, ?)",
    )
    .bind(ActionId::new().to_string())
    .bind(outcome)
    .bind(context)
    .execute(pool)
    .await
    .expect("a legacy row");
}

async fn rows(pool: &SqlitePool) -> Vec<Row> {
    sqlx::query_as("SELECT id, outcome, context FROM action_history ORDER BY id")
        .fetch_all(pool)
        .await
        .expect("history rows")
}

#[tokio::test]
async fn a_legacy_run_context_decodes_with_every_text_outcome_as_verbatim_user_text_in_step_order()
{
    let (pool, _dir) = schema_before_user_text().await;
    let original = failed_run_with_mixed_steps();
    insert_raw(
        &pool,
        &legacy_json(&original.outcome),
        &legacy_json(&original),
    )
    .await;

    apply_migrations(&pool).await.expect("migrate");

    let (_, _, context) = rows(&pool).await.remove(0);
    let decoded: ExecutionContext =
        serde_json::from_str(&context).expect("the migrated context decodes");
    assert_eq!(decoded, original);
}

#[tokio::test]
async fn a_legacy_failed_outcome_column_decodes_as_verbatim_user_text() {
    let (pool, _dir) = schema_before_user_text().await;
    let original = failed_run_with_mixed_steps();
    insert_raw(
        &pool,
        &legacy_json(&original.outcome),
        &legacy_json(&original),
    )
    .await;

    apply_migrations(&pool).await.expect("migrate");

    let (_, outcome, _) = rows(&pool).await.remove(0);
    let decoded: ExecutionOutcome =
        serde_json::from_str(&outcome).expect("the migrated outcome column decodes");
    assert_eq!(decoded, original.outcome);
}

#[tokio::test]
async fn rows_holding_text_that_is_not_json_are_left_untouched() {
    let (pool, _dir) = schema_before_user_text().await;
    insert_raw(&pool, "Success", "{\"telemetry\": [ not json").await;
    insert_raw(&pool, "{\"Failed\": unterminated", "not json either").await;
    let before = rows(&pool).await;

    apply_migrations(&pool).await.expect("migrate");

    assert_eq!(rows(&pool).await, before);
}

#[tokio::test]
async fn running_the_rewrite_again_over_migrated_rows_changes_nothing() {
    let (pool, _dir) = schema_before_user_text().await;
    let original = failed_run_with_mixed_steps();
    insert_raw(
        &pool,
        &legacy_json(&original.outcome),
        &legacy_json(&original),
    )
    .await;
    let current = run(
        ExecutionOutcome::Success,
        vec![step(0, SubActionOutcome::Success)],
    );
    insert_raw(
        &pool,
        "Success",
        &serde_json::to_string(&current).expect("serializable"),
    )
    .await;
    apply_migrations(&pool).await.expect("migrate");
    let migrated = rows(&pool).await;

    sqlx::raw_sql(USER_TEXT_REWRITE)
        .execute(&pool)
        .await
        .expect("a second run of the rewrite");

    assert_eq!(rows(&pool).await, migrated);
}
