use std::collections::{HashSet, VecDeque};
use std::sync::LazyLock;

use forge_storage::transit::{
    ActionTransit, BUNDLE_FORMAT_VERSION, BundleDocument, GlobalTransit,
    MINIMUM_SUPPORTED_BUNDLE_VERSION, ScriptTransit, TriggerInstanceTransit,
};
use forge_storage::{
    BundleExportOutcome, BundleImportOutcome, BundleRepo, ImportMode, SkippedEntity, StorageError,
};
use forge_types::{ActionId, TriggerInstanceId};
use regex::Regex;
use serde_json::Value as JsonValue;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn epoch_ms_now() -> i64 {
    let now = OffsetDateTime::now_utc();
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

/// Non-string values and absent `name` keys are silently skipped, not an error.
fn extract_script_names_from_sub_actions(sub_actions: &JsonValue) -> Vec<String> {
    let Some(arr) = sub_actions.as_array() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for step in arr {
        let kind = step.get("kind_id").and_then(|v| v.as_str());
        if kind != Some("core.script.run_named") {
            continue;
        }
        if let Some(name) = step
            .get("config")
            .and_then(|c| c.get("name"))
            .and_then(|n| n.as_str())
        {
            names.push(name.to_owned());
        }
    }
    names
}

/// Only double-quoted literal names are matched; dynamic (variable) keys cannot be
/// statically extracted and are silently omitted.
fn extract_global_names_from_body(body: &str, re: &Regex) -> Vec<String> {
    re.captures_iter(body)
        .filter_map(|cap| cap.get(2).map(|m| m.as_str().to_owned()))
        .collect()
}

#[derive(sqlx::FromRow)]
struct BundleActionRow {
    id: String,
    name: String,
    group_name: String,
    #[allow(dead_code)]
    queue_id: String,
    enabled: i64,
    concurrent: i64,
    bypass_pause: i64,
    description: String,
    sub_actions: String,
    execution_mode: String,
}

#[derive(sqlx::FromRow)]
struct BundleScriptRow {
    id: String,
    name: String,
    body: String,
    contract_json: String,
    body_hash: String,
    enabled: i64,
    created_at: i64,
    last_modified: i64,
}

#[derive(sqlx::FromRow)]
struct BundleInstanceRow {
    id: String,
    kind_id: String,
    name: String,
    overrides: String,
    enabled: i64,
    #[allow(dead_code)]
    user_defined: i64,
    #[allow(dead_code)]
    platform_scope: String,
}

#[derive(sqlx::FromRow)]
struct BundleGlobalRow {
    name: String,
    value: String,
    #[allow(dead_code)]
    type_tag: String,
    #[allow(dead_code)]
    persisted: i64,
    reads: i64,
    writes: i64,
    #[allow(dead_code)]
    created_at: i64,
    last_modified: i64,
}

pub struct SqliteBundleRepo {
    pub(crate) pool: sqlx::SqlitePool,
}

impl SqliteBundleRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl BundleRepo for SqliteBundleRepo {
    async fn import_bundle(
        &self,
        bytes: &[u8],
        mode: ImportMode,
    ) -> Result<BundleImportOutcome, StorageError> {
        let bundle: BundleDocument =
            serde_json::from_slice(bytes).map_err(|_| StorageError::MalformedBundle)?;

        if bundle.format_version < MINIMUM_SUPPORTED_BUNDLE_VERSION {
            return Err(StorageError::BundleVersionTooOld {
                found: bundle.format_version,
                minimum_supported: MINIMUM_SUPPORTED_BUNDLE_VERSION,
            });
        }

        let mut outcome = BundleImportOutcome::default();

        if bundle.format_version > BUNDLE_FORMAT_VERSION {
            outcome.warnings.push(format!(
                "bundle format version {} is newer than this install's supported version {}; \
                 some fields may be ignored",
                bundle.format_version, BUNDLE_FORMAT_VERSION
            ));
        }

        match mode {
            ImportMode::ReplaceConfirm => {
                import_replace(&self.pool, bundle, &mut outcome).await?;
            }
            ImportMode::MergeAdd => {
                import_merge(&self.pool, bundle, &mut outcome).await?;
            }
        }

        Ok(outcome)
    }

    async fn export_bundle(
        &self,
        action_ids: &[ActionId],
        include_orphan_globals: bool,
    ) -> Result<BundleExportOutcome, StorageError> {
        let mut doc = BundleDocument::new();
        let mut warnings = Vec::new();

        let mut collected_action_ids: HashSet<String> = HashSet::new();
        let mut collected_script_names: HashSet<String> = HashSet::new();
        let mut collected_global_names: HashSet<String> = HashSet::new();
        let mut collected_trigger_instance_ids: HashSet<String> = HashSet::new();

        enum WorkItem {
            Action(String),
            Script(String),
        }
        let mut worklist: VecDeque<WorkItem> = action_ids
            .iter()
            .map(|id| WorkItem::Action(id.to_string()))
            .collect();

        while let Some(item) = worklist.pop_front() {
            match item {
                WorkItem::Action(action_id_str) => {
                    if collected_action_ids.contains(&action_id_str) {
                        continue;
                    }

                    let row: Option<BundleActionRow> = sqlx::query_as(
                        "SELECT id, name, group_name, queue_id, enabled, concurrent, bypass_pause, \
                         description, sub_actions, execution_mode \
                         FROM actions WHERE id = ?",
                    )
                    .bind(&action_id_str)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(SqliteStorageError::Sqlx)?;

                    let Some(row) = row else {
                        warnings.push(format!(
                            "action '{}' not found in database; skipped",
                            action_id_str
                        ));
                        continue;
                    };

                    let sub_actions: JsonValue =
                        serde_json::from_str(&row.sub_actions).unwrap_or(JsonValue::Array(vec![]));

                    let script_names = extract_script_names_from_sub_actions(&sub_actions);
                    for sn in script_names {
                        if !collected_script_names.contains(&sn) {
                            worklist.push_back(WorkItem::Script(sn));
                        }
                    }

                    let ti_rows: Vec<(String,)> = sqlx::query_as(
                        "SELECT ti.id FROM trigger_instances ti \
                         JOIN action_trigger_instances ati ON ati.trigger_instance_id = ti.id \
                         WHERE ati.action_id = ? AND ti.user_defined = 1",
                    )
                    .bind(&action_id_str)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(SqliteStorageError::Sqlx)?;

                    for (ti_id,) in ti_rows {
                        collected_trigger_instance_ids.insert(ti_id);
                    }

                    collected_action_ids.insert(action_id_str.clone());

                    doc.actions.push(ActionTransit {
                        id: parse_action_id(&row.id)?,
                        name: row.name,
                        group: if row.group_name.is_empty() {
                            None
                        } else {
                            Some(row.group_name)
                        },
                        enabled: row.enabled != 0,
                        concurrent: row.concurrent != 0,
                        bypass_pause: row.bypass_pause != 0,
                        execution_mode: row.execution_mode,
                        description: if row.description.is_empty() {
                            None
                        } else {
                            Some(row.description)
                        },
                        sub_actions,
                        // actions has no created_at/last_modified columns; transit's String
                        // fields (not Option) mean absent is represented as empty, not omitted.
                        created_at: String::new(),
                        last_modified: String::new(),
                    });
                }

                WorkItem::Script(script_name) => {
                    if collected_script_names.contains(&script_name) {
                        continue;
                    }
                    collected_script_names.insert(script_name.clone());

                    let row: Option<BundleScriptRow> = sqlx::query_as(
                        "SELECT id, name, body, contract_json, body_hash, enabled, created_at, \
                         last_modified FROM scripts WHERE name = ?",
                    )
                    .bind(&script_name)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(SqliteStorageError::Sqlx)?;

                    let Some(row) = row else {
                        warnings.push(format!(
                            "script '{}' referenced by a sub-action but not found in database; skipped",
                            script_name
                        ));
                        continue;
                    };

                    let nested_names = extract_script_names_from_body_text(&row.body);
                    for nn in nested_names {
                        if !collected_script_names.contains(&nn) {
                            worklist.push_back(WorkItem::Script(nn));
                        }
                    }

                    let global_names = extract_global_names_from_body(&row.body, &GLOBAL_ACCESS_RE);
                    for gn in global_names {
                        collected_global_names.insert(gn);
                    }

                    let contract: JsonValue = serde_json::from_str(&row.contract_json)
                        .unwrap_or(JsonValue::Object(Default::default()));

                    doc.scripts.push(ScriptTransit {
                        id: parse_script_id(&row.id)?,
                        name: row.name,
                        body: row.body,
                        enabled: row.enabled != 0,
                        contract,
                        // Not recomputed here: the DB recomputes on every save, so the stored
                        // value is already authoritative for the current body.
                        body_hash: row.body_hash,
                        created_at: ms_to_iso(row.created_at),
                        last_modified: ms_to_iso(row.last_modified),
                    });
                }
            }
        }

        for ti_id_str in &collected_trigger_instance_ids {
            let row: Option<BundleInstanceRow> = sqlx::query_as(
                "SELECT id, kind_id, name, overrides, enabled, user_defined, platform_scope \
                 FROM trigger_instances WHERE id = ? AND user_defined = 1",
            )
            .bind(ti_id_str)
            .fetch_optional(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

            if let Some(row) = row {
                let overrides: JsonValue = serde_json::from_str(&row.overrides)
                    .unwrap_or(JsonValue::Object(Default::default()));
                doc.trigger_instances.push(TriggerInstanceTransit {
                    id: parse_trigger_instance_id(&row.id)?,
                    kind_id: row.kind_id,
                    name: row.name,
                    enabled: row.enabled != 0,
                    overrides,
                });
            }
        }

        for global_name in &collected_global_names {
            let row: Option<BundleGlobalRow> = sqlx::query_as(
                "SELECT name, value, type_tag, persisted, reads, writes, created_at, last_modified \
                 FROM globals WHERE name = ? AND persisted = 1",
            )
            .bind(global_name)
            .fetch_optional(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

            if let Some(global_row) = row
                && let Some(transit) = decode_global_row(global_row)?
            {
                doc.globals.push(transit);
            }
        }

        if include_orphan_globals {
            let all_persisted: Vec<BundleGlobalRow> = sqlx::query_as(
                "SELECT name, value, type_tag, persisted, reads, writes, created_at, last_modified \
                 FROM globals WHERE persisted = 1 ORDER BY name",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

            for global_row in all_persisted {
                if !collected_global_names.contains(global_row.name.as_str())
                    && let Some(transit) = decode_global_row(global_row)?
                {
                    doc.globals.push(transit);
                }
            }
        }

        Ok(BundleExportOutcome {
            document: doc,
            warnings,
        })
    }
}

async fn import_replace(
    pool: &sqlx::SqlitePool,
    bundle: BundleDocument,
    outcome: &mut BundleImportOutcome,
) -> Result<(), StorageError> {
    let now_ms = epoch_ms_now();
    let mut tx = pool.begin().await.map_err(SqliteStorageError::Sqlx)?;

    // Wipes actions/trigger_instances/scripts/persisted globals only; credentials,
    // settings, user_globals, and event_log survive a ReplaceConfirm import.
    sqlx::query("DELETE FROM actions")
        .execute(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    sqlx::query("DELETE FROM trigger_instances WHERE user_defined = 1")
        .execute(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    sqlx::query("DELETE FROM scripts")
        .execute(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    sqlx::query("DELETE FROM globals WHERE persisted = 1")
        .execute(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    insert_actions(&mut tx, &bundle.actions, outcome).await?;
    insert_trigger_instances(&mut tx, &bundle.trigger_instances, outcome, &mut Vec::new()).await?;
    insert_scripts(&mut tx, &bundle.scripts, outcome, now_ms).await?;
    insert_globals(&mut tx, &bundle.globals, outcome, now_ms).await?;

    tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
    Ok(())
}

async fn import_merge(
    pool: &sqlx::SqlitePool,
    bundle: BundleDocument,
    outcome: &mut BundleImportOutcome,
) -> Result<(), StorageError> {
    let now_ms = epoch_ms_now();
    let mut tx = pool.begin().await.map_err(SqliteStorageError::Sqlx)?;

    merge_actions(&mut tx, &bundle.actions, outcome).await?;
    merge_trigger_instances(&mut tx, &bundle.trigger_instances, outcome).await?;
    merge_scripts(&mut tx, &bundle.scripts, outcome, now_ms).await?;
    merge_globals(&mut tx, &bundle.globals, outcome, now_ms).await?;

    for ti in &bundle.trigger_instances {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM trigger_instances WHERE kind_id = ? AND user_defined = 0",
        )
        .bind(&ti.kind_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        if count == 0 {
            outcome.warnings.push(format!(
                "trigger '{}' references unknown template '{}'; will not fire until \
                 the matching platform crate is added",
                ti.name, ti.kind_id
            ));
        }
    }

    tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
    Ok(())
}

async fn insert_actions(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    actions: &[ActionTransit],
    outcome: &mut BundleImportOutcome,
) -> Result<(), StorageError> {
    for a in actions {
        let id_str = a.id.to_string();
        let group_name = a.group.as_deref().unwrap_or("");
        let description = a.description.as_deref().unwrap_or("");
        let sub_actions_str =
            serde_json::to_string(&a.sub_actions).map_err(StorageError::Serialization)?;
        let enabled: i64 = i64::from(a.enabled);
        let concurrent: i64 = i64::from(a.concurrent);
        let bypass_pause: i64 = i64::from(a.bypass_pause);

        sqlx::query(
            "INSERT INTO actions \
             (id, name, group_name, queue_id, enabled, concurrent, bypass_pause, \
              description, sub_actions, execution_mode) \
             VALUES (?, ?, ?, '00000000000000000000000000', ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id_str)
        .bind(&a.name)
        .bind(group_name)
        .bind(enabled)
        .bind(concurrent)
        .bind(bypass_pause)
        .bind(description)
        .bind(&sub_actions_str)
        .bind(&a.execution_mode)
        .execute(&mut **tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        outcome.actions_inserted += 1;
    }
    Ok(())
}

async fn insert_trigger_instances(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    instances: &[TriggerInstanceTransit],
    outcome: &mut BundleImportOutcome,
    warnings: &mut Vec<String>,
) -> Result<(), StorageError> {
    for ti in instances {
        let id_str = ti.id.to_string();
        let overrides_str =
            serde_json::to_string(&ti.overrides).map_err(StorageError::Serialization)?;
        let enabled: i64 = i64::from(ti.enabled);

        sqlx::query(
            "INSERT INTO trigger_instances \
             (id, kind_id, name, overrides, enabled, user_defined, platform_scope) \
             VALUES (?, ?, ?, ?, ?, 1, '\"any\"')",
        )
        .bind(&id_str)
        .bind(&ti.kind_id)
        .bind(&ti.name)
        .bind(&overrides_str)
        .bind(enabled)
        .execute(&mut **tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        outcome.trigger_instances_inserted += 1;

        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM trigger_instances WHERE kind_id = ? AND user_defined = 0",
        )
        .bind(&ti.kind_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        if count == 0 {
            warnings.push(format!(
                "trigger '{}' references unknown template '{}'; will not fire until \
                 the matching platform crate is added",
                ti.name, ti.kind_id
            ));
        }
    }
    Ok(())
}

async fn insert_scripts(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    scripts: &[ScriptTransit],
    outcome: &mut BundleImportOutcome,
    now_ms: i64,
) -> Result<(), StorageError> {
    for s in scripts {
        let id_str = s.id.to_string();
        let contract_str =
            serde_json::to_string(&s.contract).map_err(StorageError::Serialization)?;
        let enabled: i64 = i64::from(s.enabled);

        sqlx::query(
            "INSERT INTO scripts \
             (id, name, body, contract_json, body_hash, enabled, created_at, last_modified) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id_str)
        .bind(&s.name)
        .bind(&s.body)
        .bind(&contract_str)
        .bind(&s.body_hash)
        .bind(enabled)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&mut **tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        outcome.scripts_inserted += 1;
    }
    Ok(())
}

async fn insert_globals(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    globals: &[GlobalTransit],
    outcome: &mut BundleImportOutcome,
    now_ms: i64,
) -> Result<(), StorageError> {
    for g in globals {
        let value_str = serde_json::to_string(&g.value).map_err(StorageError::Serialization)?;
        let type_tag = variant_type_tag(&g.value);

        sqlx::query(
            "INSERT INTO globals \
             (name, value, type_tag, persisted, reads, writes, created_at, last_modified) \
             VALUES (?, ?, ?, 1, 0, 0, ?, ?)",
        )
        .bind(&g.name)
        .bind(&value_str)
        .bind(type_tag)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&mut **tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        outcome.globals_inserted += 1;
    }
    Ok(())
}

async fn merge_actions(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    actions: &[ActionTransit],
    outcome: &mut BundleImportOutcome,
) -> Result<(), StorageError> {
    for a in actions {
        let id_str = a.id.to_string();

        let existing: Option<(String,)> = sqlx::query_as("SELECT name FROM actions WHERE id = ?")
            .bind(&id_str)
            .fetch_optional(&mut **tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        if let Some((local_name,)) = existing {
            outcome.actions_skipped.push(SkippedEntity {
                bundle_display_name: a.name.clone(),
                local_display_name: local_name,
            });
            continue;
        }

        let group_name = a.group.as_deref().unwrap_or("");
        let description = a.description.as_deref().unwrap_or("");
        let sub_actions_str =
            serde_json::to_string(&a.sub_actions).map_err(StorageError::Serialization)?;
        let enabled: i64 = i64::from(a.enabled);
        let concurrent: i64 = i64::from(a.concurrent);
        let bypass_pause: i64 = i64::from(a.bypass_pause);

        sqlx::query(
            "INSERT INTO actions \
             (id, name, group_name, queue_id, enabled, concurrent, bypass_pause, \
              description, sub_actions, execution_mode) \
             VALUES (?, ?, ?, '00000000000000000000000000', ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id_str)
        .bind(&a.name)
        .bind(group_name)
        .bind(enabled)
        .bind(concurrent)
        .bind(bypass_pause)
        .bind(description)
        .bind(&sub_actions_str)
        .bind(&a.execution_mode)
        .execute(&mut **tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        outcome.actions_inserted += 1;
    }
    Ok(())
}

async fn merge_trigger_instances(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    instances: &[TriggerInstanceTransit],
    outcome: &mut BundleImportOutcome,
) -> Result<(), StorageError> {
    for ti in instances {
        let id_str = ti.id.to_string();

        let existing: Option<(String,)> =
            sqlx::query_as("SELECT name FROM trigger_instances WHERE id = ?")
                .bind(&id_str)
                .fetch_optional(&mut **tx)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        if let Some((local_name,)) = existing {
            outcome.trigger_instances_skipped.push(SkippedEntity {
                bundle_display_name: ti.name.clone(),
                local_display_name: local_name,
            });
            continue;
        }

        let overrides_str =
            serde_json::to_string(&ti.overrides).map_err(StorageError::Serialization)?;
        let enabled: i64 = i64::from(ti.enabled);

        sqlx::query(
            "INSERT INTO trigger_instances \
             (id, kind_id, name, overrides, enabled, user_defined, platform_scope) \
             VALUES (?, ?, ?, ?, ?, 1, '\"any\"')",
        )
        .bind(&id_str)
        .bind(&ti.kind_id)
        .bind(&ti.name)
        .bind(&overrides_str)
        .bind(enabled)
        .execute(&mut **tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        outcome.trigger_instances_inserted += 1;
    }
    Ok(())
}

async fn merge_scripts(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    scripts: &[ScriptTransit],
    outcome: &mut BundleImportOutcome,
    now_ms: i64,
) -> Result<(), StorageError> {
    for s in scripts {
        let existing: Option<(String,)> = sqlx::query_as("SELECT name FROM scripts WHERE name = ?")
            .bind(&s.name)
            .fetch_optional(&mut **tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        if let Some((local_name,)) = existing {
            outcome.scripts_skipped.push(SkippedEntity {
                bundle_display_name: s.name.clone(),
                local_display_name: local_name,
            });
            continue;
        }

        let id_str = s.id.to_string();
        let contract_str =
            serde_json::to_string(&s.contract).map_err(StorageError::Serialization)?;
        let enabled: i64 = i64::from(s.enabled);

        sqlx::query(
            "INSERT INTO scripts \
             (id, name, body, contract_json, body_hash, enabled, created_at, last_modified) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id_str)
        .bind(&s.name)
        .bind(&s.body)
        .bind(&contract_str)
        .bind(&s.body_hash)
        .bind(enabled)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&mut **tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        outcome.scripts_inserted += 1;
    }
    Ok(())
}

async fn merge_globals(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    globals: &[GlobalTransit],
    outcome: &mut BundleImportOutcome,
    now_ms: i64,
) -> Result<(), StorageError> {
    for g in globals {
        let existing: Option<(String,)> = sqlx::query_as("SELECT name FROM globals WHERE name = ?")
            .bind(&g.name)
            .fetch_optional(&mut **tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        if let Some((local_name,)) = existing {
            outcome.globals_skipped.push(SkippedEntity {
                bundle_display_name: g.name.clone(),
                local_display_name: local_name,
            });
            continue;
        }

        let value_str = serde_json::to_string(&g.value).map_err(StorageError::Serialization)?;
        let type_tag = variant_type_tag(&g.value);

        sqlx::query(
            "INSERT INTO globals \
             (name, value, type_tag, persisted, reads, writes, created_at, last_modified) \
             VALUES (?, ?, ?, 1, 0, 0, ?, ?)",
        )
        .bind(&g.name)
        .bind(&value_str)
        .bind(type_tag)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&mut **tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        outcome.globals_inserted += 1;
    }
    Ok(())
}

// Both patterns are compile-time string constants; Regex::new can only fail on
// malformed syntax, which would be a programming error caught in CI.
#[allow(clippy::expect_used)]
static RUN_NAMED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"run_named\s*\(\s*"([^"]+)""#).expect("static regex"));

#[allow(clippy::expect_used)]
static GLOBAL_ACCESS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"forge::globals::(get|set|incr|del)\s*\(\s*"([^"]+)""#).expect("static regex")
});

/// Matches `run_named("name")` regardless of any `forge::scripts::` prefix.
fn extract_script_names_from_body_text(body: &str) -> Vec<String> {
    RUN_NAMED_RE
        .captures_iter(body)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_owned()))
        .collect()
}

fn ms_to_iso(ms: i64) -> String {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .map(|t| t.to_string())
        .unwrap_or_default()
}

fn decode_global_row(row: BundleGlobalRow) -> Result<Option<GlobalTransit>, StorageError> {
    let value: forge_types::Variant = serde_json::from_str(&row.value).map_err(|e| {
        StorageError::Parse(format!("invalid global value for '{}': {e}", row.name))
    })?;

    let last_modified = OffsetDateTime::from_unix_timestamp_nanos(
        row.last_modified as i128 * 1_000_000,
    )
    .map_err(|e| {
        StorageError::Parse(format!(
            "invalid last_modified timestamp for global '{}': {e}",
            row.name
        ))
    })?;

    Ok(Some(GlobalTransit {
        name: row.name,
        value,
        persisted: true,
        last_modified,
        reads: row.reads.max(0) as u64,
        writes: row.writes.max(0) as u64,
    }))
}

fn variant_type_tag(v: &forge_types::Variant) -> &'static str {
    use forge_types::Variant;
    match v {
        Variant::Int(_) => "int",
        Variant::Float(_) => "float",
        Variant::Bool(_) => "bool",
        Variant::String(_) => "string",
        Variant::Datetime(_) => "datetime",
        Variant::Array(_) => "array",
        Variant::Object(_) => "object",
    }
}

fn parse_action_id(s: &str) -> Result<ActionId, StorageError> {
    serde_json::from_str::<ActionId>(&format!("\"{s}\""))
        .map_err(|e| StorageError::Parse(format!("invalid action id '{s}': {e}")))
}

fn parse_trigger_instance_id(s: &str) -> Result<TriggerInstanceId, StorageError> {
    serde_json::from_str::<TriggerInstanceId>(&format!("\"{s}\""))
        .map_err(|e| StorageError::Parse(format!("invalid trigger_instance id '{s}': {e}")))
}

fn parse_script_id(s: &str) -> Result<forge_types::ScriptId, StorageError> {
    serde_json::from_str::<forge_types::ScriptId>(&format!("\"{s}\""))
        .map_err(|e| StorageError::Parse(format!("invalid script id '{s}': {e}")))
}
