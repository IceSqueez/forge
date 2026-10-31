#![allow(clippy::expect_used)]
mod common;
use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_THE_MOVE: i64 = 65;
const SWITCHES: [(&str, &str); 2] = [
    ("midi.enabled", "integration.enabled.midi"),
    ("hotkey.enabled", "integration.enabled.hotkey"),
];

async fn migrated_with(rows: &[(&str, &str)]) -> (SqlitePool, TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = common::database_url(&dir);
    let pool = connect(&url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_THE_MOVE, &pool)
        .await
        .expect("the schema before the move");
    for (key, value) in rows {
        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&pool)
            .await
            .expect("a pre-migration row");
    }
    apply_migrations(&pool).await.expect("migrate");
    (pool, dir)
}

async fn stored(pool: &SqlitePool, key: &str) -> Option<(String, String, String)> {
    sqlx::query_as("SELECT value, portability, disclosure FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .expect("a row")
}

#[tokio::test]
async fn a_retired_switch_moves_to_its_integration_with_the_family_classes() {
    for (retired, current) in SWITCHES {
        for (value, moved) in [("true", "true"), ("false", "false"), (" true ", "true")] {
            let (pool, _dir) = migrated_with(&[(retired, value)]).await;

            assert_eq!(
                stored(&pool, current).await,
                Some((
                    moved.to_owned(),
                    "portable".to_owned(),
                    "verbatim".to_owned()
                )),
                "{retired} = {value:?}"
            );
            assert_eq!(stored(&pool, retired).await, None, "{retired}");
        }
    }
}

#[tokio::test]
async fn a_stored_choice_wins_over_the_retired_switch_which_is_still_removed() {
    for (retired, current) in SWITCHES {
        let (pool, _dir) = migrated_with(&[(retired, "false"), (current, "true")]).await;

        assert_eq!(
            stored(&pool, current).await.map(|row| row.0).as_deref(),
            Some("true"),
            "{retired}"
        );
        assert_eq!(stored(&pool, retired).await, None, "{retired}");
    }
}

#[tokio::test]
async fn an_invalid_retired_value_is_removed_without_being_moved() {
    for (retired, current) in SWITCHES {
        for value in ["maybe", "", "truee"] {
            let (pool, _dir) = migrated_with(&[(retired, value)]).await;

            assert_eq!(stored(&pool, current).await, None, "{retired} = {value:?}");
            assert_eq!(stored(&pool, retired).await, None, "{retired} = {value:?}");
        }
    }
}

#[tokio::test]
async fn a_store_without_retired_switches_gains_no_integration_rows() {
    let (pool, _dir) = migrated_with(&[]).await;

    for (_, current) in SWITCHES {
        assert_eq!(stored(&pool, current).await, None, "{current}");
    }
}
