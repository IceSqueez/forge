#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::path::PathBuf;

use forge_storage::ActionRepo;
use forge_storage_sqlite::{SqliteActionRepo, apply_migrations, connect};
use forge_types::ActionId;
use sqlx::SqlitePool;

const CASES: &[(&str, &str)] = &[
    ("nested.stored.json", "nested.stored.json"),
    (
        "malformed_nested.stored.json",
        "malformed_nested.stored.json",
    ),
    (
        "legacy_flags_absent.stored.json",
        "legacy_flags_absent.resaved.json",
    ),
];

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/step_tree")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_default()
}

async fn stored_steps(pool: &SqlitePool, id: ActionId) -> String {
    sqlx::query_scalar("SELECT sub_actions FROM actions WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(pool)
        .await
        .expect("the stored step tree")
}

async fn resaved(pool: &SqlitePool, stored: &str) -> String {
    let id = ActionId::new();
    sqlx::query(
        "INSERT INTO actions (id, name, queue_id, sub_actions) VALUES (?, 'golden', '00000000000000000000000000', ?)",
    )
    .bind(id.to_string())
    .bind(stored)
    .execute(pool)
    .await
    .expect("insert the stored tree");
    let repo = SqliteActionRepo::new(pool.clone());

    let action = repo
        .get(id)
        .await
        .expect("the stored tree decodes")
        .expect("the action exists");
    repo.save(&action).await.expect("the action saves");

    stored_steps(pool, id).await
}

#[tokio::test]
async fn step_trees_reload_and_resave_to_the_golden_bytes() {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");

    let mut drifted = Vec::new();
    for (case, golden_case) in CASES {
        let stored = fixture(case);
        let golden = fixture(golden_case);

        let resaved = resaved(&pool, stored.trim_end_matches('\n')).await;

        if resaved != golden.trim_end_matches('\n') {
            drifted.push(format!("{case}:\n  golden:  {golden}  resaved: {resaved}"));
        }
    }
    assert!(
        drifted.is_empty(),
        "step trees drifted:\n{}",
        drifted.join("\n")
    );
}
