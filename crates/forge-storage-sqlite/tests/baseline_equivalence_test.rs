#![allow(clippy::expect_used)]

mod legacy_fixture;

use forge_storage_sqlite::legacy_chain::LEGACY_CHAIN;
use forge_storage_sqlite::{BASELINE_VERSION, MIGRATIONS};
use legacy_fixture::{LEGACY_HEAD, fixture_texts, legacy_db, sha384_hex, table_rows, user_tables};
use sqlx::SqlitePool;
use tempfile::TempDir;

type ForeignKeyRow = (i64, String, String, Option<String>, String, String, String);
type IndexKeyRow = (i64, i64, Option<String>, bool, Option<String>, bool);

struct Pair {
    legacy: SqlitePool,
    baseline: SqlitePool,
    _dir: TempDir,
}

async fn built_pair() -> Pair {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let legacy = legacy_db(&dir.path().join("legacy.db"), LEGACY_HEAD).await;
    let baseline =
        forge_storage_sqlite::connect(&legacy_fixture::db_url(&dir.path().join("baseline.db")))
            .await
            .expect("a baseline database");
    MIGRATIONS
        .run_to(BASELINE_VERSION, &baseline)
        .await
        .expect("the baseline applies");
    Pair {
        legacy,
        baseline,
        _dir: dir,
    }
}

async fn table_sql(pool: &SqlitePool, table: &str) -> String {
    sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?")
        .bind(table)
        .fetch_one(pool)
        .await
        .expect("the CREATE text of a table")
}

async fn columns(pool: &SqlitePool) -> Vec<String> {
    let mut out = Vec::new();
    for table in user_tables(pool).await {
        let rows: Vec<(i64, String, String, i64, Option<String>, i64)> = sqlx::query_as(
            "SELECT cid, name, type, \"notnull\", dflt_value, pk FROM pragma_table_info(?) ORDER BY cid",
        )
        .bind(&table)
        .fetch_all(pool)
        .await
        .expect("table_info");
        out.extend(rows.into_iter().map(|row| format!("{table}: {row:?}")));
    }
    out
}

async fn foreign_keys(pool: &SqlitePool) -> Vec<String> {
    let mut out = Vec::new();
    for table in user_tables(pool).await {
        let rows: Vec<ForeignKeyRow> = sqlx::query_as(
            "SELECT seq, \"table\", \"from\", \"to\", on_update, on_delete, \"match\"
                 FROM pragma_foreign_key_list(?)",
        )
        .bind(&table)
        .fetch_all(pool)
        .await
        .expect("foreign_key_list");
        out.extend(rows.into_iter().map(|row| format!("{table}: {row:?}")));
    }
    out.sort();
    out
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn partial_predicate(index_sql: &str) -> String {
    let upper = index_sql.to_uppercase();
    upper
        .rfind(" WHERE ")
        .map(|at| compact(&index_sql[at + " WHERE ".len()..]))
        .unwrap_or_default()
}

async fn indexes(pool: &SqlitePool) -> Vec<String> {
    let mut out = Vec::new();
    for table in user_tables(pool).await {
        let listed: Vec<(String, bool, String, bool)> =
            sqlx::query_as("SELECT name, \"unique\", origin, partial FROM pragma_index_list(?)")
                .bind(&table)
                .fetch_all(pool)
                .await
                .expect("index_list");
        for (name, unique, origin, partial) in listed {
            let keys: Vec<IndexKeyRow> =
                sqlx::query_as(
                    "SELECT seqno, cid, name, \"desc\", coll, key FROM pragma_index_xinfo(?) ORDER BY seqno",
                )
                .bind(&name)
                .fetch_all(pool)
                .await
                .expect("index_xinfo");
            let predicate = if partial {
                let sql: String = sqlx::query_scalar(
                    "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?",
                )
                .bind(&name)
                .fetch_one(pool)
                .await
                .expect("the CREATE text of a partial index");
                partial_predicate(&sql)
            } else {
                String::new()
            };
            out.push(format!(
                "{table}.{name} unique={unique} origin={origin} where={predicate} keys={keys:?}"
            ));
        }
    }
    out.sort();
    out
}

fn check_clauses(create_sql: &str) -> Vec<String> {
    let compacted = compact(create_sql);
    let upper = compacted.to_uppercase();
    let mut clauses = Vec::new();
    let mut from = 0;
    while let Some(found) = upper[from..].find("CHECK(") {
        let open = from + found + "CHECK".len();
        let mut depth = 0usize;
        let mut close = open;
        for (offset, ch) in compacted[open..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = open + offset;
                        break;
                    }
                }
                _ => {}
            }
        }
        clauses.push(compacted[open..=close].to_owned());
        from = close;
    }
    clauses.sort();
    clauses
}

async fn checks(pool: &SqlitePool) -> Vec<String> {
    let mut out = Vec::new();
    for table in user_tables(pool).await {
        let sql = table_sql(pool, &table).await;
        out.extend(
            check_clauses(&sql)
                .into_iter()
                .map(|clause| format!("{table}: {clause}")),
        );
    }
    out
}

async fn autoincrement_tables(pool: &SqlitePool) -> Vec<String> {
    let mut out = Vec::new();
    for table in user_tables(pool).await {
        if table_sql(pool, &table)
            .await
            .to_uppercase()
            .contains("AUTOINCREMENT")
        {
            out.push(table);
        }
    }
    out
}

async fn triggers_and_views(pool: &SqlitePool) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT type, name, tbl_name FROM sqlite_master WHERE type IN ('trigger', 'view') ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("triggers and views")
}

async fn seed_rows(pool: &SqlitePool) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for table in user_tables(pool).await {
        out.push((table.clone(), table_rows(pool, &table, true).await));
    }
    out
}

#[tokio::test]
async fn baseline_creates_the_same_tables_and_columns_as_the_legacy_chain() {
    let pair = built_pair().await;

    let legacy = columns(&pair.legacy).await;

    assert!(!legacy.is_empty());
    assert_eq!(columns(&pair.baseline).await, legacy);
}

#[tokio::test]
async fn baseline_declares_the_same_foreign_keys_as_the_legacy_chain() {
    let pair = built_pair().await;

    let legacy = foreign_keys(&pair.legacy).await;

    assert!(!legacy.is_empty());
    assert_eq!(foreign_keys(&pair.baseline).await, legacy);
}

#[tokio::test]
async fn baseline_creates_the_same_indexes_and_partial_predicates_as_the_legacy_chain() {
    let pair = built_pair().await;

    let legacy = indexes(&pair.legacy).await;

    assert!(!legacy.is_empty());
    assert_eq!(indexes(&pair.baseline).await, legacy);
}

#[tokio::test]
async fn baseline_declares_the_same_check_constraints_as_the_legacy_chain() {
    let pair = built_pair().await;

    let legacy = checks(&pair.legacy).await;

    assert!(!legacy.is_empty());
    assert_eq!(checks(&pair.baseline).await, legacy);
}

#[tokio::test]
async fn baseline_marks_the_same_tables_autoincrement_as_the_legacy_chain() {
    let pair = built_pair().await;

    let legacy = autoincrement_tables(&pair.legacy).await;

    assert!(!legacy.is_empty());
    assert_eq!(autoincrement_tables(&pair.baseline).await, legacy);
}

#[tokio::test]
async fn baseline_has_the_same_triggers_and_views_as_the_legacy_chain() {
    let pair = built_pair().await;

    assert_eq!(
        triggers_and_views(&pair.baseline).await,
        triggers_and_views(&pair.legacy).await
    );
}

#[tokio::test]
async fn baseline_seeds_the_same_rows_as_the_legacy_chain() {
    let pair = built_pair().await;

    let legacy = seed_rows(&pair.legacy).await;

    assert!(!legacy.is_empty());
    assert_eq!(seed_rows(&pair.baseline).await, legacy);
}

#[test]
fn frozen_legacy_chain_lists_exactly_the_shipped_versions_in_order() {
    let shipped: Vec<i64> = fixture_texts()
        .iter()
        .map(|(version, _)| *version)
        .collect();
    let frozen: Vec<i64> = LEGACY_CHAIN.iter().map(|m| m.version).collect();

    assert_eq!(frozen, shipped);
}

#[test]
fn frozen_lf_checksums_equal_the_sha384_of_the_shipped_files() {
    for (version, text) in fixture_texts() {
        let frozen = LEGACY_CHAIN
            .iter()
            .find(|m| m.version == version)
            .expect("a frozen entry for every shipped file");
        assert_eq!(frozen.checksum_lf, sha384_hex(&text), "version {version}");
    }
}

#[test]
fn frozen_crlf_checksums_equal_the_sha384_of_the_shipped_files_with_crlf_endings() {
    for (version, text) in fixture_texts() {
        let frozen = LEGACY_CHAIN
            .iter()
            .find(|m| m.version == version)
            .expect("a frozen entry for every shipped file");
        assert_eq!(
            frozen.checksum_crlf,
            sha384_hex(&text.replace('\n', "\r\n")),
            "version {version}"
        );
    }
}
