use async_trait::async_trait;
use forge_storage::{QueueRepo, StorageError};
use forge_types::{Queue, QueueId};
use serde_json;

use crate::error::SqliteStorageError;

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
    paused: i64,
}

fn decode_row(row: QueueRow) -> Result<Queue, SqliteStorageError> {
    let id = parse_queue_id(&row.id)?;
    Ok(Queue {
        id,
        name: row.name,
        description: row.description,
        concurrency: row.concurrency.max(1) as u32,
        paused: row.paused != 0,
    })
}

pub struct SqliteQueueRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteQueueRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl QueueRepo for SqliteQueueRepo {
    async fn list(&self) -> Result<Vec<Queue>, StorageError> {
        let rows: Vec<QueueRow> = sqlx::query_as(
            "SELECT id, name, description, concurrency, paused FROM queues ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }

    async fn get(&self, id: QueueId) -> Result<Option<Queue>, StorageError> {
        let id_str = id.to_string();
        let row: Option<QueueRow> = sqlx::query_as(
            "SELECT id, name, description, concurrency, paused FROM queues WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|row| decode_row(row).map_err(StorageError::from))
            .transpose()
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<Queue>, StorageError> {
        let row: Option<QueueRow> = sqlx::query_as(
            "SELECT id, name, description, concurrency, paused FROM queues WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|row| decode_row(row).map_err(StorageError::from))
            .transpose()
    }

    async fn save(&self, queue: &Queue) -> Result<(), StorageError> {
        let id_str = queue.id.to_string();
        let concurrency = i64::from(queue.concurrency.max(1));
        let blocking: i64 = if queue.is_serial() { 1 } else { 0 };
        let paused: i64 = if queue.paused { 1 } else { 0 };

        sqlx::query(
            "INSERT INTO queues (id, name, description, blocking, concurrency, paused)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 name        = excluded.name,
                 description = excluded.description,
                 blocking    = excluded.blocking,
                 concurrency = excluded.concurrency,
                 paused      = excluded.paused",
        )
        .bind(&id_str)
        .bind(&queue.name)
        .bind(&queue.description)
        .bind(blocking)
        .bind(concurrency)
        .bind(paused)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn delete(&self, id: QueueId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM queues WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }
}
