use std::collections::HashMap;
use std::time::Instant;

use forge_registry::rename_step_kinds;
use forge_storage::rename_favorite_ids;
use forge_storage::settings::core_settings;
use forge_types::SubActionStep;
use serde_json::Value;
use sqlx::SqliteConnection;
use sqlx::migrate::Migration;

use crate::error::SqliteStorageError;
use crate::migrations::MIGRATIONS;

pub const KIND_ID_RENAME_VERSION: i64 = 58;

const TRIGGER_STORE: &str = "trigger";
const SUB_ACTION_STORE: &str = "sub_action";
const TRIGGER_KIND_POINTER: &str = "/metadata/trigger_kind";
const TELEMETRY_FIELD: &str = "telemetry";
const STEP_KIND_FIELD: &str = "kind";

#[derive(Default)]
struct KindRenames {
    triggers: HashMap<String, String>,
    sub_actions: HashMap<String, String>,
}

impl KindRenames {
    fn current_trigger(&self, id: &str) -> Option<String> {
        self.triggers.get(id).cloned()
    }

    fn current_sub_action(&self, id: &str) -> Option<String> {
        self.sub_actions.get(id).cloned()
    }
}

#[derive(Default)]
struct RenameCounts {
    actions: u64,
    history: u64,
    favorites: u64,
    skipped: u64,
}

pub async fn is_pending(pool: &sqlx::SqlitePool) -> Result<bool, SqliteStorageError> {
    let mut conn = pool.acquire().await.map_err(SqliteStorageError::Sqlx)?;
    pending(&mut conn).await
}

pub async fn apply(pool: &sqlx::SqlitePool) -> Result<(), SqliteStorageError> {
    let migration = MIGRATIONS
        .iter()
        .find(|migration| migration.version == KIND_ID_RENAME_VERSION)
        .ok_or_else(|| failed("the embedded migrations lack the kind id rename".to_owned()))?;
    let started = Instant::now();
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(SqliteStorageError::Sqlx)?;
    if !pending(&mut tx).await? {
        return Ok(());
    }
    sqlx::raw_sql(migration.sql.clone())
        .execute(&mut *tx)
        .await
        .map_err(|e| failed(e.to_string()))?;
    let renames = load_renames(&mut tx).await?;
    let mut counts = RenameCounts::default();
    rename_action_steps(&mut tx, &renames, &mut counts).await?;
    rename_history(&mut tx, &renames, &mut counts).await?;
    rename_favorites(&mut tx, &renames, &mut counts).await?;
    sqlx::query("DROP TABLE temp.retired_kind_ids")
        .execute(&mut *tx)
        .await
        .map_err(|e| failed(e.to_string()))?;
    record_applied(&mut tx, migration, started).await?;
    tx.commit().await.map_err(|e| failed(e.to_string()))?;
    tracing::info!(
        version = KIND_ID_RENAME_VERSION,
        actions = counts.actions,
        history = counts.history,
        favorites = counts.favorites,
        skipped = counts.skipped,
        "renamed retired kind ids in stored data"
    );
    Ok(())
}

async fn pending(conn: &mut SqliteConnection) -> Result<bool, SqliteStorageError> {
    let applied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = ?")
            .bind(KIND_ID_RENAME_VERSION)
            .fetch_one(&mut *conn)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
    Ok(applied == 0)
}

async fn load_renames(conn: &mut SqliteConnection) -> Result<KindRenames, SqliteStorageError> {
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT store, retired_id, current_id FROM temp.retired_kind_ids")
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| failed(e.to_string()))?;
    let mut renames = KindRenames::default();
    for (store, retired, current) in rows {
        match store.as_str() {
            TRIGGER_STORE => renames.triggers.insert(retired, current),
            SUB_ACTION_STORE => renames.sub_actions.insert(retired, current),
            _ => return Err(failed(format!("unknown rename store {store}"))),
        };
    }
    Ok(renames)
}

async fn rename_action_steps(
    conn: &mut SqliteConnection,
    renames: &KindRenames,
    counts: &mut RenameCounts,
) -> Result<(), SqliteStorageError> {
    let rows: Vec<(String, String)> =
        sqlx::query_as(rows_naming_a_retired_id("actions", "sub_actions"))
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| failed(e.to_string()))?;
    let current_of = |id: &str| renames.current_sub_action(id);
    for (id, stored) in rows {
        let Ok(mut steps) = serde_json::from_str::<Vec<SubActionStep>>(&stored) else {
            tracing::warn!(action = %id, "kept an unreadable step list while renaming kind ids");
            counts.skipped += 1;
            continue;
        };
        let mut changed = false;
        for step in &mut steps {
            changed |= rename_step_kinds(&mut step.kind_id, &mut step.config, &current_of);
        }
        if !changed {
            continue;
        }
        let encoded = serde_json::to_string(&steps).map_err(|e| failed(e.to_string()))?;
        sqlx::query("UPDATE actions SET sub_actions = ? WHERE id = ?")
            .bind(encoded)
            .bind(&id)
            .execute(&mut *conn)
            .await
            .map_err(|e| failed(e.to_string()))?;
        counts.actions += 1;
    }
    Ok(())
}

async fn rename_history(
    conn: &mut SqliteConnection,
    renames: &KindRenames,
    counts: &mut RenameCounts,
) -> Result<(), SqliteStorageError> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as(rows_naming_a_retired_id("action_history", "context"))
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| failed(e.to_string()))?;
    for (id, stored) in rows {
        let Ok(mut context) = serde_json::from_str::<Value>(&stored) else {
            counts.skipped += 1;
            continue;
        };
        if !rename_context(&mut context, renames) {
            continue;
        }
        let encoded = serde_json::to_string(&context).map_err(|e| failed(e.to_string()))?;
        sqlx::query("UPDATE action_history SET context = ? WHERE id = ?")
            .bind(encoded)
            .bind(id)
            .execute(&mut *conn)
            .await
            .map_err(|e| failed(e.to_string()))?;
        counts.history += 1;
    }
    Ok(())
}

fn rename_context(context: &mut Value, renames: &KindRenames) -> bool {
    let mut changed = false;
    if let Some(slot) = context.pointer_mut(TRIGGER_KIND_POINTER) {
        changed |= rename_kind_slot(slot, |kind| renames.current_trigger(kind));
    }
    if let Some(steps) = context
        .get_mut(TELEMETRY_FIELD)
        .and_then(Value::as_array_mut)
    {
        for step in steps {
            if let Some(slot) = step.get_mut(STEP_KIND_FIELD) {
                changed |= rename_kind_slot(slot, |kind| renames.current_sub_action(kind));
            }
        }
    }
    changed
}

fn rename_kind_slot(slot: &mut Value, current_of: impl Fn(&str) -> Option<String>) -> bool {
    let Some(current) = slot.as_str().and_then(current_of) else {
        return false;
    };
    *slot = Value::String(current);
    true
}

async fn rename_favorites(
    conn: &mut SqliteConnection,
    renames: &KindRenames,
    counts: &mut RenameCounts,
) -> Result<(), SqliteStorageError> {
    let lists = [
        (
            core_settings::PICKER_FAVORITES_SUB_ACTIONS.name(),
            &renames.sub_actions,
        ),
        (
            core_settings::PICKER_FAVORITES_TRIGGERS.name(),
            &renames.triggers,
        ),
    ];
    for (key, current_by_retired) in lists {
        let stored: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| failed(e.to_string()))?;
        let Some(renamed) = stored.and_then(|stored| {
            rename_favorite_ids(&stored, &|id| current_by_retired.get(id).cloned())
        }) else {
            continue;
        };
        sqlx::query("UPDATE settings SET value = ? WHERE key = ?")
            .bind(renamed)
            .bind(key)
            .execute(&mut *conn)
            .await
            .map_err(|e| failed(e.to_string()))?;
        counts.favorites += 1;
    }
    Ok(())
}

async fn record_applied(
    conn: &mut SqliteConnection,
    migration: &Migration,
    started: Instant,
) -> Result<(), SqliteStorageError> {
    let elapsed_ns = i64::try_from(started.elapsed().as_nanos()).unwrap_or(i64::MAX);
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(migration.version)
    .bind(migration.description.as_ref())
    .bind(true)
    .bind(migration.checksum.as_ref())
    .bind(elapsed_ns)
    .execute(&mut *conn)
    .await
    .map_err(|e| failed(e.to_string()))?;
    Ok(())
}

fn rows_naming_a_retired_id(table: &str, column: &str) -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(format!(
        "SELECT id, {column} FROM {table} WHERE EXISTS (
             SELECT 1 FROM temp.retired_kind_ids r
             WHERE instr({column}, '\"' || r.retired_id || '\"') > 0
         )"
    ))
}

fn failed(reason: String) -> SqliteStorageError {
    SqliteStorageError::Migration {
        migration: format!("{KIND_ID_RENAME_VERSION} kind id rename"),
        reason,
    }
}
