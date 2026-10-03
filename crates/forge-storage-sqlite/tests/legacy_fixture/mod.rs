#![allow(dead_code, clippy::expect_used)]

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha384};
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;

pub const LEGACY_HEAD: i64 = 46;

pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy_migrations")
}

pub fn fixture_texts() -> Vec<(i64, String)> {
    let mut texts: Vec<(i64, String)> = std::fs::read_dir(fixture_dir())
        .expect("the legacy fixture directory")
        .map(|entry| {
            let path = entry.expect("a fixture entry").path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("a UTF-8 fixture name")
                .to_owned();
            let version = name
                .split('_')
                .next()
                .and_then(|prefix| prefix.parse::<i64>().ok())
                .expect("a numeric version prefix");
            let text = std::fs::read_to_string(&path)
                .expect("a readable fixture")
                .replace("\r\n", "\n");
            (version, text)
        })
        .collect();
    texts.sort_by_key(|(version, _)| *version);
    texts
}

pub fn sha384_hex(text: &str) -> String {
    Sha384::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn db_url(path: &Path) -> String {
    format!("sqlite://{}", path.display())
}

pub async fn legacy_migrator() -> Migrator {
    Migrator::new(fixture_dir())
        .await
        .expect("the legacy fixture chain resolves")
}

pub async fn legacy_db(path: &Path, up_to: i64) -> SqlitePool {
    let pool = forge_storage_sqlite::connect(&db_url(path))
        .await
        .expect("a legacy database");
    legacy_migrator()
        .await
        .run_to(up_to, &pool)
        .await
        .expect("the legacy chain applies");
    pool
}

pub async fn rewrite_journal_to_crlf(pool: &SqlitePool) {
    for (version, text) in fixture_texts() {
        let crlf = text.replace('\n', "\r\n");
        let digest: Vec<u8> = Sha384::digest(crlf.as_bytes()).to_vec();
        sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
            .bind(digest)
            .bind(version)
            .execute(pool)
            .await
            .expect("rewrite a journal checksum");
    }
}

pub type JournalRow = (i64, String, String, bool, Vec<u8>, i64);

pub async fn journal(pool: &SqlitePool) -> Vec<JournalRow> {
    sqlx::query_as(
        "SELECT version, description, installed_on, success, checksum, execution_time
         FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .expect("read the journal")
}

pub async fn user_tables(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite!_%' ESCAPE '!' AND name <> '_sqlx_migrations'
         ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("list tables")
}

pub async fn column_names(pool: &SqlitePool, table: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
        .bind(table)
        .fetch_all(pool)
        .await
        .expect("read columns")
}

pub async fn table_rows(pool: &SqlitePool, table: &str, mask_timestamps: bool) -> Vec<String> {
    let projection = column_names(pool, table)
        .await
        .iter()
        .map(|column| {
            if mask_timestamps && column.ends_with("_at") {
                format!("CASE WHEN \"{column}\" IS NULL THEN 'NULL' ELSE '<time>' END")
            } else {
                format!("quote(\"{column}\")")
            }
        })
        .collect::<Vec<_>>()
        .join(" || '|' || ");
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {projection} FROM \"{table}\" ORDER BY 1"
    )))
    .fetch_all(pool)
    .await
    .expect("read table rows")
}

pub async fn data_fingerprint(pool: &SqlitePool) -> Vec<(String, Vec<String>)> {
    let mut fingerprint = Vec::new();
    for table in user_tables(pool).await {
        let rows = table_rows(pool, &table, false).await;
        fingerprint.push((table, rows));
    }
    fingerprint
}
