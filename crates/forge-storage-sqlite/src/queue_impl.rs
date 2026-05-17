use async_trait::async_trait;
use forge_storage::{QueueRepo, StorageError};
use forge_types::{Queue, QueueId};
use serde_json;

use crate::error::SqliteStorageError;

fn parse_queue_id(s: &str) -> Result<QueueId, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid queue id '{s}': {e}")))
}

fn decode_row(id_str: String, name: String, blocking: i64) -> Result<Queue, SqliteStorageError> {
    let id = parse_queue_id(&id_str)?;
    Ok(Queue {
        id,
        name,
        blocking: blocking != 0,
    })
}

type QueueRow = (String, String, i64);

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
        let rows: Vec<QueueRow> =
            sqlx::query_as("SELECT id, name, blocking FROM queues ORDER BY name")
                .fetch_all(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|(id, name, blocking)| decode_row(id, name, blocking).map_err(StorageError::from))
            .collect()
    }

    async fn get(&self, id: QueueId) -> Result<Option<Queue>, StorageError> {
        let id_str = id.to_string();
        let row: Option<QueueRow> =
            sqlx::query_as("SELECT id, name, blocking FROM queues WHERE id = ?")
                .bind(&id_str)
                .fetch_optional(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        row.map(|(id, name, blocking)| decode_row(id, name, blocking).map_err(StorageError::from))
            .transpose()
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<Queue>, StorageError> {
        let row: Option<QueueRow> =
            sqlx::query_as("SELECT id, name, blocking FROM queues WHERE name = ?")
                .bind(name)
                .fetch_optional(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        row.map(|(id, name, blocking)| decode_row(id, name, blocking).map_err(StorageError::from))
            .transpose()
    }

    async fn save(&self, queue: &Queue) -> Result<(), StorageError> {
        let id_str = queue.id.to_string();
        let blocking: i64 = if queue.blocking { 1 } else { 0 };

        sqlx::query(
            "INSERT INTO queues (id, name, blocking)
             VALUES (?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 name     = excluded.name,
                 blocking = excluded.blocking",
        )
        .bind(&id_str)
        .bind(&queue.name)
        .bind(blocking)
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
