use std::num::NonZeroU32;

use async_trait::async_trait;
use forge_storage::{QueueRepo, StorageError};
use forge_types::{DEFAULT_QUEUE_ID, Queue, QueueId};

use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

fn parse_queue_id(s: &str) -> Result<QueueId, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid queue id '{s}': {e}")))
}

#[derive(sqlx::FromRow)]
struct QueueRow {
    id: String,
    name: String,
    description: String,
    concurrency: i64,
}

fn decode_row(row: QueueRow) -> Result<Queue, SqliteStorageError> {
    let id = parse_queue_id(&row.id)?;
    Ok(Queue {
        id,
        name: row.name,
        description: row.description,
        concurrency: NonZeroU32::new(row.concurrency.max(1) as u32).unwrap_or(NonZeroU32::MIN),
    })
}

const OVERLAY_ALERT_ROLE: &str = "overlay_alerts";

pub(crate) async fn upsert_queue<'e>(
    executor: impl sqlx::SqliteExecutor<'e>,
    queue: &Queue,
) -> Result<(), StorageError> {
    let id_str = queue.id.to_string();
    let concurrency = i64::from(queue.concurrency.get());
    let blocking: i64 = if queue.is_serial() { 1 } else { 0 };

    sqlx::query(
        "INSERT INTO queues (id, name, description, blocking, concurrency)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
             name        = excluded.name,
             description = excluded.description,
             blocking    = excluded.blocking,
             concurrency = excluded.concurrency",
    )
    .bind(&id_str)
    .bind(&queue.name)
    .bind(&queue.description)
    .bind(blocking)
    .bind(concurrency)
    .execute(executor)
    .await
    .map_err(SqliteStorageError::Sqlx)?;

    Ok(())
}

pub(crate) async fn delete_queue<'e>(
    executor: impl sqlx::SqliteExecutor<'e>,
    id: QueueId,
) -> Result<bool, StorageError> {
    if id == DEFAULT_QUEUE_ID {
        return Err(StorageError::DefaultQueueUndeletable);
    }
    let result = sqlx::query("DELETE FROM queues WHERE id = ?")
        .bind(id.to_string())
        .execute(executor)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    Ok(result.rows_affected() > 0)
}

pub(crate) async fn mark_overlay_alert_queue<'e>(
    executor: impl sqlx::SqliteExecutor<'e>,
    id: QueueId,
) -> Result<(), StorageError> {
    sqlx::query("UPDATE queues SET role = ? WHERE id = ?")
        .bind(OVERLAY_ALERT_ROLE)
        .bind(id.to_string())
        .execute(executor)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    Ok(())
}

pub(crate) async fn assign_overlay_alert_queue(
    conn: &mut sqlx::SqliteConnection,
    id: QueueId,
) -> Result<(), StorageError> {
    sqlx::query("UPDATE queues SET role = NULL WHERE role = ? AND id <> ?")
        .bind(OVERLAY_ALERT_ROLE)
        .bind(id.to_string())
        .execute(&mut *conn)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    mark_overlay_alert_queue(conn, id).await
}

pub(crate) async fn claim_overlay_alert_queue<'e>(
    executor: impl sqlx::SqliteExecutor<'e>,
    id: QueueId,
) -> Result<(), StorageError> {
    sqlx::query(
        "UPDATE queues SET role = ?
         WHERE id = ? AND NOT EXISTS (SELECT 1 FROM queues WHERE role = ?)",
    )
    .bind(OVERLAY_ALERT_ROLE)
    .bind(id.to_string())
    .bind(OVERLAY_ALERT_ROLE)
    .execute(executor)
    .await
    .map_err(SqliteStorageError::Sqlx)?;

    Ok(())
}

pub struct SqliteQueueRepo {
    db: SqlitePools,
}

impl SqliteQueueRepo {
    pub fn new(db: impl Into<SqlitePools>) -> Self {
        Self { db: db.into() }
    }
}

#[async_trait]
impl QueueRepo for SqliteQueueRepo {
    async fn list(&self) -> Result<Vec<Queue>, StorageError> {
        let rows: Vec<QueueRow> =
            sqlx::query_as("SELECT id, name, description, concurrency FROM queues ORDER BY name")
                .fetch_all(self.db.reader())
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }

    async fn get(&self, id: QueueId) -> Result<Option<Queue>, StorageError> {
        let id_str = id.to_string();
        let row: Option<QueueRow> =
            sqlx::query_as("SELECT id, name, description, concurrency FROM queues WHERE id = ?")
                .bind(&id_str)
                .fetch_optional(self.db.reader())
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        row.map(|row| decode_row(row).map_err(StorageError::from))
            .transpose()
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<Queue>, StorageError> {
        let row: Option<QueueRow> =
            sqlx::query_as("SELECT id, name, description, concurrency FROM queues WHERE name = ?")
                .bind(name)
                .fetch_optional(self.db.reader())
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        row.map(|row| decode_row(row).map_err(StorageError::from))
            .transpose()
    }

    async fn save(&self, queue: &Queue) -> Result<(), StorageError> {
        upsert_queue(self.db.writer(), queue).await
    }

    async fn delete(&self, id: QueueId) -> Result<bool, StorageError> {
        delete_queue(self.db.writer(), id).await
    }

    async fn overlay_alert_queue(
        &self,
        adoptable_name: &str,
    ) -> Result<Option<Queue>, StorageError> {
        let row: Option<QueueRow> = sqlx::query_as(
            "SELECT id, name, description, concurrency FROM queues
             WHERE role = ?
                OR (name = ? AND NOT EXISTS (SELECT 1 FROM queues WHERE role = ?))
             ORDER BY role IS NULL, id
             LIMIT 1",
        )
        .bind(OVERLAY_ALERT_ROLE)
        .bind(adoptable_name)
        .bind(OVERLAY_ALERT_ROLE)
        .fetch_optional(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|row| decode_row(row).map_err(StorageError::from))
            .transpose()
    }

    async fn delete_reassigning_actions(&self, id: QueueId) -> Result<Option<u64>, StorageError> {
        if id == DEFAULT_QUEUE_ID {
            return Err(StorageError::DefaultQueueUndeletable);
        }
        let id_str = id.to_string();
        let mut tx = self
            .db
            .writer()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        let reassigned = sqlx::query("UPDATE actions SET queue_id = ? WHERE queue_id = ?")
            .bind(DEFAULT_QUEUE_ID.to_string())
            .bind(&id_str)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?
            .rows_affected();
        let deleted = sqlx::query("DELETE FROM queues WHERE id = ?")
            .bind(&id_str)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?
            .rows_affected();
        if deleted == 0 {
            return Ok(None);
        }
        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        Ok(Some(reassigned))
    }
}
