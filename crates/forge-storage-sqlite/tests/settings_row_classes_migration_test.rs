#![allow(clippy::expect_used, clippy::panic)]
use std::collections::BTreeMap;

use forge_storage::{SettingDisclosure, SettingPortability};
use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_CLASSES: i64 = 58;
const GOLDEN: &str = include_str!("fixtures/settings/key_catalog.tsv");
const UNKNOWN_KEYS: [&str; 6] = [
    "legacy.unknown_flag",
    "integration.enabled",
    "tts.engine_params",
    "Theme",
    "xintegration.enabled.twitch",
    "themes",
];
const FAMILY_MEMBERS: [&str; 3] = ["piper", "a.b", "ключ"];

type Classes = (String, String);

async fn migrated_from_58(rows: &[(String, String)]) -> (SqlitePool, TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}", dir.path().join("forge.db").display());
    let pool = connect(&url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_CLASSES, &pool)
        .await
        .expect("the schema before the classes");
    sqlx::query("DELETE FROM settings")
        .execute(&pool)
        .await
        .expect("clear the baseline rows");
    for (key, value) in rows {
        sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&pool)
            .await
            .expect("a pre-migration row");
    }
    apply_migrations(&pool).await.expect("migrate");
    (pool, dir)
}

async fn stored(pool: &SqlitePool) -> BTreeMap<String, (String, Classes)> {
    let rows: Vec<(String, String, String, String)> =
        sqlx::query_as("SELECT key, value, portability, disclosure FROM settings")
            .fetch_all(pool)
            .await
            .expect("rows");
    rows.into_iter()
        .map(|(key, value, portability, disclosure)| (key, (value, (portability, disclosure))))
        .collect()
}

struct Golden {
    exact: Vec<(String, Classes)>,
    prefixes: Vec<(String, Classes)>,
}

fn golden() -> Golden {
    let mut exact = Vec::new();
    let mut prefixes = Vec::new();
    for line in GOLDEN.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        let entry = (
            fields[1].to_owned(),
            (fields[2].to_owned(), fields[3].to_owned()),
        );
        match fields[0] {
            "key" => exact.push(entry),
            "prefix" => prefixes.push(entry),
            other => panic!("unknown golden row kind {other}"),
        }
    }
    Golden { exact, prefixes }
}

#[tokio::test]
async fn every_catalogued_exact_key_receives_its_catalog_classes_and_keeps_its_value() {
    let golden = golden();
    let seeded: Vec<(String, String)> = golden
        .exact
        .iter()
        .map(|(key, _)| (key.clone(), format!("value-of-{key}")))
        .collect();
    let (pool, _dir) = migrated_from_58(&seeded).await;
    let after = stored(&pool).await;

    let wrong: Vec<_> = golden
        .exact
        .iter()
        .filter(|(key, classes)| {
            after.get(key) != Some(&(format!("value-of-{key}"), classes.clone()))
        })
        .map(|(key, classes)| (key, classes, after.get(key)))
        .collect();
    assert!(wrong.is_empty(), "misclassified: {wrong:?}");
    assert_eq!(after.len(), golden.exact.len());
}

#[tokio::test]
async fn family_members_receive_the_family_classes() {
    let golden = golden();
    let mut seeded = Vec::new();
    for (prefix, _) in &golden.prefixes {
        for member in FAMILY_MEMBERS {
            seeded.push((format!("{prefix}{member}"), "v".to_owned()));
        }
    }
    let (pool, _dir) = migrated_from_58(&seeded).await;
    let after = stored(&pool).await;

    assert!(!golden.prefixes.is_empty());
    for (prefix, classes) in &golden.prefixes {
        for member in FAMILY_MEMBERS {
            let key = format!("{prefix}{member}");
            assert_eq!(after.get(&key).map(|row| &row.1), Some(classes), "{key}");
        }
    }
}

#[tokio::test]
async fn no_exact_catalog_key_sits_under_a_family_prefix_so_exact_over_family_precedence_is_unobservable()
 {
    let golden = golden();
    for (key, _) in &golden.exact {
        for (prefix, _) in &golden.prefixes {
            assert!(!key.starts_with(prefix.as_str()), "{key} under {prefix}");
        }
    }
}

#[tokio::test]
async fn unknown_rows_stay_machine_local_and_withheld_and_are_kept_with_their_values() {
    let seeded: Vec<(String, String)> = UNKNOWN_KEYS
        .iter()
        .map(|key| ((*key).to_owned(), format!("secret-{key}")))
        .collect();
    let (pool, _dir) = migrated_from_58(&seeded).await;
    let after = stored(&pool).await;

    let fail_closed = (
        SettingPortability::MachineLocal.code().to_owned(),
        SettingDisclosure::Withheld.code().to_owned(),
    );
    for key in UNKNOWN_KEYS {
        assert_eq!(
            after.get(key),
            Some(&(format!("secret-{key}"), fail_closed.clone())),
            "{key}"
        );
    }
    assert_eq!(after.len(), UNKNOWN_KEYS.len());
}

#[tokio::test]
async fn a_row_inserted_without_classes_after_migration_is_fail_closed() {
    let (pool, _dir) = migrated_from_58(&[]).await;
    sqlx::query("INSERT INTO settings (key, value) VALUES ('later', 'x')")
        .execute(&pool)
        .await
        .expect("insert");

    let after = stored(&pool).await;

    assert_eq!(
        after.get("later").map(|row| &row.1),
        Some(&("machine_local".to_owned(), "withheld".to_owned()))
    );
}

#[tokio::test]
async fn the_check_constraints_reject_unknown_class_codes_on_insert_and_update() {
    let (pool, _dir) = migrated_from_58(&[("theme".to_owned(), "mocha".to_owned())]).await;

    let results = [
        sqlx::query("INSERT INTO settings (key, value, portability) VALUES ('a', 'x', 'global')")
            .execute(&pool)
            .await,
        sqlx::query("INSERT INTO settings (key, value, disclosure) VALUES ('b', 'x', 'secret')")
            .execute(&pool)
            .await,
        sqlx::query("UPDATE settings SET portability = 'Portable' WHERE key = 'theme'")
            .execute(&pool)
            .await,
        sqlx::query("UPDATE settings SET disclosure = '' WHERE key = 'theme'")
            .execute(&pool)
            .await,
    ];

    assert!(results.iter().all(Result::is_err));
    let after = stored(&pool).await;
    assert_eq!(
        after.get("theme").map(|row| &row.1),
        Some(&("portable".to_owned(), "verbatim".to_owned()))
    );
    assert_eq!(after.len(), 1);
}

#[tokio::test]
async fn every_valid_class_code_pair_is_accepted_by_the_check_constraints() {
    let (pool, _dir) = migrated_from_58(&[]).await;
    for portability in ["portable", "machine_local", "repository_owned"] {
        for disclosure in ["verbatim", "kind", "presence", "withheld"] {
            sqlx::query(
                "INSERT INTO settings (key, value, portability, disclosure) VALUES (?, 'x', ?, ?)",
            )
            .bind(format!("{portability}/{disclosure}"))
            .bind(portability)
            .bind(disclosure)
            .execute(&pool)
            .await
            .expect("a valid class pair");
        }
    }
    assert_eq!(stored(&pool).await.len(), 12);
}
