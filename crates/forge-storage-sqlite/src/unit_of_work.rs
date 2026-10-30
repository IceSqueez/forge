use forge_storage::{StorageError, StoreChange};
use sqlx::SqliteConnection;

use crate::action_impl::upsert_action;
use crate::error::SqliteStorageError;
use crate::globals_impl;
use crate::media_impl::{release_all_references, release_reference, retain_reference};
use crate::overlay_impl::{delete_overlay, insert_new_overlay, update_overlay};
use crate::pool::SqlitePools;
use crate::queue_impl::{delete_queue, upsert_queue};
use crate::script_impl::upsert_script;
use crate::settings_impl::{delete_row, upsert_row};
use crate::soundboard_impl::{delete_clip, upsert_clip};
use crate::trigger_instance_impl::{append_link, ignore_unique_violation, upsert_instance};
use crate::tts_filters_impl::{write_pipeline_settings, write_rules};
use crate::voice_alias_impl::{store_ignore_profile, upsert_alias};

pub(crate) async fn commit(
    pools: SqlitePools,
    changes: Vec<StoreChange>,
) -> Result<(), StorageError> {
    let mut tx = pools
        .writer()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(SqliteStorageError::Sqlx)?;
    for change in &changes {
        apply(&mut tx, change).await?;
    }
    tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
    Ok(())
}

async fn execute(
    conn: &mut SqliteConnection,
    sql: &'static str,
    id: String,
) -> Result<(), StorageError> {
    sqlx::query(sql)
        .bind(id)
        .execute(conn)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
    Ok(())
}

async fn apply(conn: &mut SqliteConnection, change: &StoreChange) -> Result<(), StorageError> {
    match change {
        StoreChange::SaveQueue(queue) => upsert_queue(conn, queue).await,
        StoreChange::DeleteQueue(id) => delete_queue(conn, *id).await.map(drop),
        StoreChange::SaveScript(record) => upsert_script(conn, record).await,
        StoreChange::RestoreScript(id) => {
            execute(
                conn,
                "UPDATE scripts SET archived_at = NULL WHERE id = ?",
                id.to_string(),
            )
            .await
        }
        StoreChange::DeleteScript(id) => {
            execute(conn, "DELETE FROM scripts WHERE id = ?", id.to_string()).await
        }
        StoreChange::SetGlobal {
            name,
            value,
            persisted,
        } => globals_impl::upsert(conn, name, value, *persisted).await,
        StoreChange::DeleteGlobal(name) => {
            execute(conn, "DELETE FROM globals WHERE name = ?", name.clone()).await
        }
        StoreChange::SaveClip(clip) => upsert_clip(conn, clip).await,
        StoreChange::DeleteClip(id) => delete_clip(conn, *id).await.map(drop),
        StoreChange::RetainMedia { referrer, blob } => retain_reference(conn, referrer, blob).await,
        StoreChange::ReleaseMedia(referrer) => release_reference(conn, referrer).await.map(drop),
        StoreChange::ReleaseAllMedia { kind, referrer_id } => {
            release_all_references(conn, *kind, referrer_id)
                .await
                .map(drop)
        }
        StoreChange::CreateOverlay(overlay) => insert_new_overlay(conn, overlay).await,
        StoreChange::SaveOverlay(definition) => update_overlay(conn, definition).await,
        StoreChange::DeleteOverlay(id) => delete_overlay(conn, id).await.map(drop),
        StoreChange::SaveTrigger(instance) => upsert_instance(conn, instance).await,
        StoreChange::RestoreTrigger(id) => {
            execute(
                conn,
                "UPDATE trigger_instances SET archived_at = NULL WHERE id = ?",
                id.to_string(),
            )
            .await
        }
        StoreChange::DeleteTrigger(id) => {
            let id = id.to_string();
            execute(
                &mut *conn,
                "DELETE FROM action_trigger_instances WHERE trigger_instance_id = ?",
                id.clone(),
            )
            .await?;
            execute(conn, "DELETE FROM trigger_instances WHERE id = ?", id).await
        }
        StoreChange::SaveAction(action) => upsert_action(conn, action).await,
        StoreChange::RestoreAction(id) => {
            execute(
                conn,
                "UPDATE actions SET archived_at = NULL WHERE id = ?",
                id.to_string(),
            )
            .await
        }
        StoreChange::DeleteAction(id) => {
            execute(conn, "DELETE FROM actions WHERE id = ?", id.to_string()).await
        }
        StoreChange::LinkTrigger { action, trigger } => {
            ignore_unique_violation(append_link(conn, *action, *trigger).await)
        }
        StoreChange::UnlinkAllTriggers(action) => {
            execute(
                conn,
                "DELETE FROM action_trigger_instances WHERE action_id = ?",
                action.to_string(),
            )
            .await
        }
        StoreChange::SetPipelineSettings(settings) => write_pipeline_settings(conn, settings).await,
        StoreChange::ReplaceFilterRules(rules) => write_rules(conn, rules).await,
        StoreChange::UpsertVoiceAlias(alias) => upsert_alias(conn, alias).await.map(drop),
        StoreChange::DeleteVoiceAlias(id) => {
            execute(conn, "DELETE FROM voice_aliases WHERE id = ?", id.0.clone()).await
        }
        StoreChange::SetIgnoreProfile(profile) => store_ignore_profile(conn, profile).await,
        StoreChange::PutSetting(row) => {
            upsert_row(conn, row.key(), row.value(), row.classes()).await
        }
        StoreChange::DeleteSetting(row) => delete_row(conn, row.key()).await.map(drop),
    }
}
