#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Regression coverage for the Discord sub-action `kind_id` rewrite
//! (registry format 1 -> 2) in `registry_migration::migrate_registry_format`.
//!
//! The blobs under test are already `SubActionStep`-shaped (format 1), so the
//! 0 -> 1 conversion pass is a no-op and the migration exercises the 1 -> 2
//! discord remap directly. Rows are inserted with `format_version = 1`
//! explicitly (the column defaults to 0) so the 1 -> 2 path is driven without
//! going through the 0 -> 1 -> 2 chain. The `actions.format_version = 0`
//! conversion path is covered by `registry_migration_test.rs`.

use forge_storage_sqlite::{connect, registry_migration};
use forge_types::{ActionId, SubActionStep, Variant};
use sqlx::SqlitePool;

async fn fresh_pool() -> SqlitePool {
    let pool = connect("sqlite::memory:").await.expect("connect");
    forge_storage_sqlite::apply_migrations(&pool)
        .await
        .expect("apply migrations");
    pool
}

/// Insert a format-1 action with a verbatim `sub_actions` JSON blob.
async fn insert_format1_action(pool: &SqlitePool, id: &ActionId, sub_actions_json: &str) {
    sqlx::query(
        "INSERT INTO actions (id, name, queue_id, sub_actions, format_version) \
         VALUES (?, 'test', '00000000000000000000000000', ?, 1)",
    )
    .bind(id.to_string())
    .bind(sub_actions_json)
    .execute(pool)
    .await
    .expect("insert format-1 action");
}

async fn read_action(pool: &SqlitePool, id: &ActionId) -> (Vec<SubActionStep>, i64) {
    let (json, version): (String, i64) =
        sqlx::query_as("SELECT sub_actions, format_version FROM actions WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(pool)
            .await
            .expect("fetch action");
    let steps: Vec<SubActionStep> = serde_json::from_str(&json).expect("parse steps");
    (steps, version)
}

#[tokio::test]
async fn legacy_discord_kind_ids_are_rewritten_to_webhook_ids() {
    // All three 1-to-1 discord remaps in one format-1 blob; config / enabled /
    // label must survive untouched.
    let remaps = [
        ("discord.post_text", "discord.webhook.send_message"),
        ("discord.post_embed", "discord.webhook.send_embed"),
        ("discord.edit_message", "discord.webhook.update_message"),
    ];

    let blob = serde_json::json!(
        remaps
            .iter()
            .enumerate()
            .map(|(i, (old, _))| serde_json::json!({
                "kind_id": old,
                "config": { "content": { "type": "string", "value": format!("msg-{i}") } },
                "enabled": i % 2 == 0,
                "label": format!("step-{i}"),
            }))
            .collect::<Vec<_>>()
    )
    .to_string();

    let pool = fresh_pool().await;
    let id = ActionId::new();
    insert_format1_action(&pool, &id, &blob).await;

    registry_migration::migrate_registry_format(&pool)
        .await
        .expect("migrate");

    let (steps, version) = read_action(&pool, &id).await;

    assert_eq!(version, 2, "format must bump to 2");
    assert_eq!(steps.len(), 3);
    for (i, (_, new_id)) in remaps.iter().enumerate() {
        assert_eq!(steps[i].kind_id, *new_id, "kind_id at index {i} remapped");
        assert_eq!(
            steps[i].config.get("content"),
            Some(&Variant::String(format!("msg-{i}"))),
            "config preserved for step {i}",
        );
        assert_eq!(
            steps[i].enabled,
            i % 2 == 0,
            "enabled preserved for step {i}"
        );
        assert_eq!(
            steps[i].label.as_deref(),
            Some(format!("step-{i}").as_str()),
            "label preserved for step {i}",
        );
    }
}

#[tokio::test]
async fn non_discord_step_is_byte_identical_after_discord_remap() {
    // A discord step alongside a non-discord step: only the discord kind_id
    // changes; the other step round-trips byte-for-byte.
    let pool = fresh_pool().await;
    let id = ActionId::new();
    insert_format1_action(
        &pool,
        &id,
        r#"[{"kind_id":"discord.post_text","config":{},"enabled":true,"label":null},{"kind_id":"core.logic.wait","config":{"ms":{"type":"int","value":500}},"enabled":true,"label":null}]"#,
    )
    .await;

    registry_migration::migrate_registry_format(&pool)
        .await
        .expect("migrate");

    let (steps, version) = read_action(&pool, &id).await;

    assert_eq!(version, 2);
    assert_eq!(steps[0].kind_id, "discord.webhook.send_message");

    let wait = &steps[1];
    assert_eq!(wait.kind_id, "core.logic.wait");
    assert_eq!(wait.config.get("ms"), Some(&Variant::Int(500)));
    assert!(wait.enabled);
    assert_eq!(wait.label, None);
}

#[tokio::test]
async fn already_canonical_discord_ids_are_left_untouched_but_bumped() {
    // discord.webhook.* ids added after the rename are not in the remap table;
    // they survive verbatim while the row still bumps to format 2.
    let pool = fresh_pool().await;
    let id = ActionId::new();
    insert_format1_action(
        &pool,
        &id,
        r#"[{"kind_id":"discord.webhook.send_file","config":{},"enabled":true,"label":null},{"kind_id":"discord.webhook.delete_message","config":{},"enabled":true,"label":null}]"#,
    )
    .await;

    registry_migration::migrate_registry_format(&pool)
        .await
        .expect("migrate");

    let (steps, version) = read_action(&pool, &id).await;

    assert_eq!(version, 2);
    assert_eq!(steps[0].kind_id, "discord.webhook.send_file");
    assert_eq!(steps[1].kind_id, "discord.webhook.delete_message");
}

#[tokio::test]
async fn second_migration_call_is_a_noop_for_discord_remap() {
    // After the first run bumps to format 2, the format-1 query matches nothing,
    // so a second call leaves kind_ids and version stable.
    let pool = fresh_pool().await;
    let id = ActionId::new();
    insert_format1_action(
        &pool,
        &id,
        r#"[{"kind_id":"discord.post_embed","config":{},"enabled":true,"label":null}]"#,
    )
    .await;

    registry_migration::migrate_registry_format(&pool)
        .await
        .expect("first migrate");
    registry_migration::migrate_registry_format(&pool)
        .await
        .expect("second migrate must not error");

    let (steps, version) = read_action(&pool, &id).await;

    assert_eq!(version, 2);
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].kind_id, "discord.webhook.send_embed");
}
