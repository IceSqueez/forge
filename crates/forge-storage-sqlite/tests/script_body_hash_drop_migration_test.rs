#![allow(clippy::expect_used)]
mod common;
use forge_storage::ScriptRepo;
use forge_storage_sqlite::{MIGRATIONS, SqliteBackend, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_DROP: i64 = 56;
const VERSION_AFTER_DROP: i64 = 57;
const KEY: [u8; 32] = [0xab; 32];
const CREATED_MS: i64 = 1_700_000_000_000;
const MODIFIED_MS: i64 = 1_700_000_500_000;
const ARCHIVED_MS: i64 = 1_700_000_900_000;
const CONTRACT: &str = r#"{"inputs":[{"name":"user","kind":"string"}],"returns":"string"}"#;
const ALPHA_ID: &str = "01J0000000000000000000000A";
const BETA_ID: &str = "01J0000000000000000000000B";

type ScriptColumns = (
    String,
    String,
    String,
    Option<String>,
    i64,
    i64,
    i64,
    String,
    Option<i64>,
);

fn db_url(dir: &TempDir, file: &str) -> String {
    common::sqlite_url(&dir.path().join(file))
}

async fn schema_56_with_scripts(url: &str) -> SqlitePool {
    let pool = connect(url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_DROP, &pool)
        .await
        .expect("the schema before the drop");
    sqlx::query(
        "INSERT INTO scripts
            (id, name, body, description, enabled, created_at, last_modified, contract_json, body_hash, archived_at)
         VALUES (?, 'alpha', 'print(user);', 'greets', 1, ?, ?, ?, 'cafe01', NULL),
                (?, 'beta', 'let x = 1;', NULL, 0, ?, ?, '{}', 'cafe02', ?)",
    )
    .bind(ALPHA_ID)
    .bind(CREATED_MS)
    .bind(MODIFIED_MS)
    .bind(CONTRACT)
    .bind(BETA_ID)
    .bind(CREATED_MS)
    .bind(MODIFIED_MS)
    .bind(ARCHIVED_MS)
    .execute(&pool)
    .await
    .expect("script rows with a hash");
    pool
}

async fn migrated_from_schema_56() -> (SqlitePool, TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let pool = schema_56_with_scripts(&db_url(&dir, "forge.db")).await;
    apply_migrations(&pool).await.expect("migrate");
    (pool, dir)
}

async fn script_rows(pool: &SqlitePool) -> Vec<ScriptColumns> {
    sqlx::query_as(
        "SELECT id, name, body, description, enabled, created_at, last_modified, contract_json, archived_at
         FROM scripts ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("script rows")
}

#[tokio::test]
async fn migration_records_the_drop_in_the_journal_and_drops_only_the_hash_column() {
    let (pool, _dir) = migrated_from_schema_56().await;

    let version: Option<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE version = ?")
            .bind(VERSION_AFTER_DROP)
            .fetch_optional(&pool)
            .await
            .expect("version");
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('scripts') ORDER BY cid")
            .fetch_all(&pool)
            .await
            .expect("columns");

    assert_eq!(
        (version, columns),
        (
            Some(VERSION_AFTER_DROP),
            [
                "id",
                "name",
                "body",
                "description",
                "enabled",
                "created_at",
                "last_modified",
                "contract_json",
                "archived_at"
            ]
            .map(str::to_owned)
            .to_vec()
        )
    );
}

#[tokio::test]
async fn migration_keeps_every_other_column_of_every_script_row() {
    let (pool, _dir) = migrated_from_schema_56().await;

    let rows = script_rows(&pool).await;

    assert_eq!(
        rows,
        vec![
            (
                ALPHA_ID.to_owned(),
                "alpha".to_owned(),
                "print(user);".to_owned(),
                Some("greets".to_owned()),
                1,
                CREATED_MS,
                MODIFIED_MS,
                CONTRACT.to_owned(),
                None
            ),
            (
                BETA_ID.to_owned(),
                "beta".to_owned(),
                "let x = 1;".to_owned(),
                None,
                0,
                CREATED_MS,
                MODIFIED_MS,
                "{}".to_owned(),
                Some(ARCHIVED_MS)
            ),
        ]
    );
}

#[tokio::test]
async fn migration_keeps_both_script_indexes_serving_indexed_lookups() {
    let (pool, _dir) = migrated_from_schema_56().await;

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'scripts'
         AND name IN ('scripts_enabled', 'scripts_archived') ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("indexes");
    let enabled_lookup = sqlx::query_scalar::<_, String>(
        "SELECT id FROM scripts INDEXED BY scripts_enabled WHERE enabled = 1",
    )
    .fetch_all(&pool)
    .await;
    let archived_lookup = sqlx::query_scalar::<_, String>(
        "SELECT id FROM scripts INDEXED BY scripts_archived WHERE archived_at IS NOT NULL",
    )
    .fetch_all(&pool)
    .await;

    assert_eq!(
        (indexes, enabled_lookup.ok(), archived_lookup.ok()),
        (
            vec!["scripts_archived".to_owned(), "scripts_enabled".to_owned()],
            Some(vec![ALPHA_ID.to_owned()]),
            Some(vec![BETA_ID.to_owned()]),
        )
    );
}

#[tokio::test]
async fn migration_keeps_script_names_unique() {
    let (pool, _dir) = migrated_from_schema_56().await;

    let duplicate = sqlx::query(
        "INSERT INTO scripts (id, name, body, enabled, created_at, last_modified)
         VALUES ('01J0000000000000000000000C', 'alpha', '', 1, 0, 0)",
    )
    .execute(&pool)
    .await;

    assert!(
        matches!(&duplicate, Err(sqlx::Error::Database(e)) if e.is_unique_violation()),
        "expected a unique violation, got {duplicate:?}"
    );
}

#[tokio::test]
async fn schema_56_database_copied_into_place_opens_migrates_and_loads_its_scripts() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let source_url = db_url(&dir, "backup.db");
    let source = schema_56_with_scripts(&source_url).await;
    source.close().await;
    let restored = dir.path().join("restored.db");
    std::fs::copy(dir.path().join("backup.db"), &restored).expect("place the backup");

    let media = dir.path().join("media");
    let backend = SqliteBackend::open_for_test(&db_url(&dir, "restored.db"), KEY, media, None)
        .await
        .expect("open the restored database");
    let scripts = backend.list().await.expect("list scripts");
    let alpha = backend
        .get_by_name("alpha")
        .await
        .expect("get")
        .expect("alpha");

    assert_eq!(
        (
            scripts.len(),
            alpha.body.as_str(),
            alpha.enabled,
            alpha.contract.inputs.len(),
        ),
        (1, "print(user);", true, 1)
    );
}
