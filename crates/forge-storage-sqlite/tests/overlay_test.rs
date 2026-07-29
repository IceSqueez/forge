#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use forge_storage::{OverlayConfig, OverlayCredential, OverlayId, OverlayRepo, StorageError};
use forge_storage_sqlite::{SqliteOverlayRepo, apply_migrations, connect};
use forge_types::Variant;
use sqlx::SqlitePool;
use time::OffsetDateTime;

async fn fresh() -> (SqliteOverlayRepo, SqlitePool) {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");
    (SqliteOverlayRepo::new(pool.clone()), pool)
}

async fn row_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM overlays")
        .fetch_one(pool)
        .await
        .expect("count overlays")
}

#[tokio::test]
async fn create_mints_the_identity_slug_from_the_display_name() {
    for (display_name, expected_slug) in [
        ("Alert Box", "alert-box"),
        ("UPPER Case", "upper-case"),
        ("  Chat   Overlay!!  ", "chat-overlay"),
        ("a-b", "a-b"),
        ("Hello Мир", "hello"),
        ("Привіт Світ", "overlay"),
        ("!!!", "overlay"),
        ("", "overlay"),
    ] {
        let (repo, _pool) = fresh().await;

        let created = repo
            .create(display_name, "forge.chat", 1)
            .await
            .expect("create overlay");

        assert_eq!(
            created.id.as_str(),
            expected_slug,
            "display_name {display_name:?} minted the wrong slug"
        );
    }
}

#[tokio::test]
async fn colliding_slugs_take_the_first_free_numeric_suffix() {
    let (repo, _pool) = fresh().await;

    let first = repo
        .create("Alert Box", "forge.chat", 1)
        .await
        .expect("first");
    let second = repo
        .create("Alert Box", "forge.chat", 1)
        .await
        .expect("second");
    let third = repo
        .create("Alert Box", "forge.chat", 1)
        .await
        .expect("third");

    assert_eq!(first.id.as_str(), "alert-box");
    assert_eq!(second.id.as_str(), "alert-box-2");
    assert_eq!(third.id.as_str(), "alert-box-3");

    assert!(repo.delete(&second.id).await.expect("delete second"));

    let refilled = repo
        .create("Alert Box", "forge.chat", 1)
        .await
        .expect("refill the freed suffix");

    assert_eq!(
        refilled.id.as_str(),
        "alert-box-2",
        "a freed suffix must be reused before allocating a higher one"
    );
}

#[tokio::test]
async fn create_positions_past_the_highest_survivor_after_deletions() {
    let (repo, _pool) = fresh().await;

    let first = repo.create("One", "forge.chat", 1).await.expect("one");
    let second = repo.create("Two", "forge.chat", 1).await.expect("two");
    let third = repo.create("Three", "forge.chat", 1).await.expect("three");
    assert_eq!(third.position, 2);

    assert!(repo.delete(&first.id).await.expect("delete first"));
    assert!(repo.delete(&second.id).await.expect("delete second"));

    let fourth = repo.create("Four", "forge.chat", 1).await.expect("four");

    assert_eq!(
        fourth.position, 3,
        "a new overlay must sort after every survivor, not reuse a freed slot"
    );
}

#[tokio::test]
async fn renaming_through_save_keeps_the_minted_identity() {
    let (repo, pool) = fresh().await;

    let mut definition = repo
        .create("Alert Box", "forge.chat", 1)
        .await
        .expect("create overlay");
    let minted_id = definition.id.clone();

    definition.display_name = "Chat Overlay".to_owned();
    repo.save(&definition).await.expect("save rename");

    assert_eq!(
        row_count(&pool).await,
        1,
        "rename must not create a new row"
    );

    let reloaded = repo
        .get(&minted_id)
        .await
        .expect("get by minted id")
        .expect("row still addressable by the minted id");
    assert_eq!(reloaded.display_name, "Chat Overlay");

    assert!(
        repo.get(&OverlayId::new("chat-overlay"))
            .await
            .expect("get by the renamed slug")
            .is_none(),
        "identity must not be re-minted from the new display name"
    );
}

#[tokio::test]
async fn save_round_trips_an_unknown_kind_with_a_sparse_config() {
    let (repo, _pool) = fresh().await;

    let mut definition = repo
        .create("Round Trip", "forge.chat", 1)
        .await
        .expect("create overlay");

    let mut config = OverlayConfig::new();
    config.insert("count".to_owned(), Variant::Int(-42));
    config.insert("ratio".to_owned(), Variant::Float(1.5));
    config.insert("visible".to_owned(), Variant::Bool(false));
    config.insert(
        "label".to_owned(),
        Variant::String("Привіт \"quoted\" \\ back".to_owned()),
    );
    config.insert(
        "at".to_owned(),
        Variant::Datetime(
            OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("fixed timestamp"),
        ),
    );
    config.insert(
        "items".to_owned(),
        Variant::Array(vec![Variant::Int(1), Variant::String(String::new())]),
    );
    config.insert(
        "nested".to_owned(),
        Variant::Object(BTreeMap::from([("k".to_owned(), Variant::Bool(true))])),
    );

    definition.kind_id = "vendor.unregistered.kind".to_owned();
    definition.config = config.clone();
    definition.config_schema_version = u32::MAX;
    definition.generator_version = 7;
    definition.source_overrides = vec!["index.html".to_owned(), String::new()];
    definition.enabled = false;
    repo.save(&definition).await.expect("save definition");

    let reloaded = repo
        .get(&definition.id)
        .await
        .expect("get overlay")
        .expect("overlay present");

    assert_eq!(reloaded.kind_id, "vendor.unregistered.kind");
    assert_eq!(reloaded.config, config);
    assert_eq!(reloaded.config_schema_version, u32::MAX);
    assert_eq!(reloaded.generator_version, 7);
    assert_eq!(reloaded.source_overrides, definition.source_overrides);
    assert!(!reloaded.enabled);
}

#[tokio::test]
async fn save_preserves_created_at_and_restamps_updated_at() {
    let (repo, _pool) = fresh().await;

    let mut definition = repo
        .create("Timestamps", "forge.chat", 1)
        .await
        .expect("create overlay");
    let created_at = definition.created_at;

    definition.updated_at = OffsetDateTime::from_unix_timestamp(0).expect("epoch");
    repo.save(&definition).await.expect("save definition");

    let reloaded = repo
        .get(&definition.id)
        .await
        .expect("get overlay")
        .expect("overlay present");

    assert_eq!(
        reloaded.created_at.unix_timestamp(),
        created_at.unix_timestamp(),
        "created_at must survive an upsert"
    );
    assert!(
        reloaded.updated_at >= created_at,
        "updated_at must be restamped by the repo, not taken from the caller"
    );
}

#[tokio::test]
async fn get_by_credential_matches_the_exact_token_only() {
    let (repo, _pool) = fresh().await;

    let target = repo
        .create("Target", "forge.chat", 1)
        .await
        .expect("target");
    repo.create("Other", "forge.chat", 1).await.expect("other");

    let hit = repo
        .get_by_credential(&target.credential)
        .await
        .expect("lookup by credential")
        .expect("exact credential resolves");
    assert_eq!(hit.id, target.id);

    let full = target.credential.as_str();
    for near_miss in [
        String::new(),
        "not-a-credential".to_owned(),
        full[..full.len() - 1].to_owned(),
        format!("{full}0"),
        full.to_uppercase(),
    ] {
        assert!(
            repo.get_by_credential(&OverlayCredential::new(near_miss.clone()))
                .await
                .expect("lookup by near-miss credential")
                .is_none(),
            "credential {near_miss:?} must not resolve an overlay"
        );
    }
}

#[tokio::test]
async fn saving_a_duplicate_credential_is_rejected_without_leaking_the_token() {
    let (repo, pool) = fresh().await;

    let first = repo.create("First", "forge.chat", 1).await.expect("first");
    let mut second = repo
        .create("Second", "forge.chat", 1)
        .await
        .expect("second");

    second.credential = first.credential.clone();
    let err = repo
        .save(&second)
        .await
        .expect_err("the unique index must reject a shared credential");

    assert!(
        matches!(err, StorageError::Connection { .. }),
        "unexpected error variant: {err:?}"
    );
    assert!(
        !err.to_string().contains(first.credential.as_str()),
        "the credential must not appear in the error message"
    );
    assert_eq!(row_count(&pool).await, 2);
}

#[tokio::test]
async fn set_enabled_flips_the_stored_flag_and_reports_a_hit() {
    let (repo, _pool) = fresh().await;

    let definition = repo
        .create("Toggle", "forge.chat", 1)
        .await
        .expect("create");
    assert!(definition.enabled, "a fresh overlay starts enabled");

    assert!(
        repo.set_enabled(&definition.id, false)
            .await
            .expect("set_enabled")
    );

    let reloaded = repo
        .get(&definition.id)
        .await
        .expect("get overlay")
        .expect("overlay present");
    assert!(!reloaded.enabled);
}

#[tokio::test]
async fn set_enabled_reports_a_miss_for_an_unknown_id() {
    let (repo, _pool) = fresh().await;

    let hit = repo
        .set_enabled(&OverlayId::new("does-not-exist"), true)
        .await
        .expect("set_enabled");

    assert!(!hit);
}

#[tokio::test]
async fn delete_removes_the_row_and_reports_a_hit() {
    let (repo, pool) = fresh().await;

    let definition = repo
        .create("Doomed", "forge.chat", 1)
        .await
        .expect("create");

    assert!(repo.delete(&definition.id).await.expect("delete"));
    assert_eq!(row_count(&pool).await, 0);
}

#[tokio::test]
async fn delete_reports_a_miss_for_an_unknown_id() {
    let (repo, _pool) = fresh().await;

    let hit = repo
        .delete(&OverlayId::new("does-not-exist"))
        .await
        .expect("delete");

    assert!(!hit);
}

#[tokio::test]
async fn list_orders_by_position_then_display_name() {
    let (repo, _pool) = fresh().await;

    let mut zeta = repo.create("Zeta", "forge.chat", 1).await.expect("zeta");
    let mut beta = repo.create("Beta", "forge.chat", 1).await.expect("beta");
    let mut alpha = repo.create("Alpha", "forge.chat", 1).await.expect("alpha");

    zeta.position = 0;
    beta.position = 1;
    alpha.position = 1;
    for definition in [&zeta, &beta, &alpha] {
        repo.save(definition).await.expect("save position");
    }

    let ids: Vec<String> = repo
        .list()
        .await
        .expect("list overlays")
        .into_iter()
        .map(|d| d.id.as_str().to_owned())
        .collect();

    assert_eq!(ids, vec!["zeta", "alpha", "beta"]);
}

#[tokio::test]
async fn a_corrupt_row_surfaces_a_parse_error_instead_of_panicking() {
    for (label, update_sql, value) in [
        (
            "config",
            "UPDATE overlays SET config = ? WHERE id = ?",
            "not json".to_owned(),
        ),
        (
            "source_overrides",
            "UPDATE overlays SET source_overrides = ? WHERE id = ?",
            "{}".to_owned(),
        ),
        (
            "created_at",
            "UPDATE overlays SET created_at = ? WHERE id = ?",
            i64::MAX.to_string(),
        ),
    ] {
        let (repo, pool) = fresh().await;
        let definition = repo
            .create("Corrupt", "forge.chat", 1)
            .await
            .expect("create");

        sqlx::query(update_sql)
            .bind(&value)
            .bind(definition.id.as_str())
            .execute(&pool)
            .await
            .expect("corrupt the row");

        let err = repo
            .get(&definition.id)
            .await
            .expect_err("a corrupt row must not decode");

        assert!(
            matches!(err, StorageError::Parse(_)),
            "column {label} produced {err:?}"
        );
    }
}

#[tokio::test]
async fn reapplying_migrations_leaves_existing_rows_untouched() {
    let (repo, pool) = fresh().await;

    sqlx::query("INSERT INTO settings (key, value) VALUES ('overlay_host_root', '/srv/overlays')")
        .execute(&pool)
        .await
        .expect("seed a pre-existing settings row");
    let definition = repo
        .create("Survivor", "forge.chat", 1)
        .await
        .expect("create");

    apply_migrations(&pool).await.expect("reapply migrations");

    let setting: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'overlay_host_root'")
            .fetch_optional(&pool)
            .await
            .expect("read settings");
    assert_eq!(setting.as_deref(), Some("/srv/overlays"));

    assert!(
        repo.get(&definition.id)
            .await
            .expect("get overlay")
            .is_some(),
        "an existing overlay must survive a migration re-run"
    );
}
