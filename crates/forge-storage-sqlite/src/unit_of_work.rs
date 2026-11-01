use forge_storage::{StorageError, StoreChange};
use sqlx::SqliteConnection;

use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;
use crate::repos::action::upsert_action;
use crate::repos::globals;
use crate::repos::media::{release_all_references, release_reference, retain_reference};
use crate::repos::overlay::{delete_overlay, insert_new_overlay, update_overlay};
use crate::repos::queue::{
    assign_overlay_alert_queue, claim_overlay_alert_queue, delete_queue, upsert_queue,
};
use crate::repos::script::upsert_script;
use crate::repos::settings::{delete_row, upsert_row};
use crate::repos::soundboard::{delete_clip, upsert_clip};
use crate::repos::trigger_instance::{append_link, ignore_unique_violation, upsert_instance};
use crate::repos::tts_filters::{write_pipeline_settings, write_rules};
use crate::repos::voice_alias::{store_ignore_profile, upsert_alias};

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
        StoreChange::AssignOverlayAlertQueue(id) => assign_overlay_alert_queue(conn, *id).await,
        StoreChange::ClaimOverlayAlertQueue(id) => claim_overlay_alert_queue(conn, *id).await,
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
        } => globals::upsert(conn, name, value, *persisted).await,
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
            execute(
                conn,
                "DELETE FROM voice_aliases WHERE id = ?",
                id.as_str().to_owned(),
            )
            .await
        }
        StoreChange::SetIgnoreProfile(profile) => store_ignore_profile(conn, profile).await,
        StoreChange::PutSetting(row) => {
            upsert_row(conn, row.key(), row.value(), row.classes()).await
        }
        StoreChange::DeleteSetting(row) => delete_row(conn, row.key()).await.map(drop),
    }
}
