use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_storage::{EventLogRepo, StorageError};
use forge_types::EventId;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn to_epoch_secs(dt: OffsetDateTime) -> i64 {
    dt.unix_timestamp()
}

fn from_epoch_secs(secs: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp(secs)
        .map_err(|e| SqliteStorageError::Decode(format!("timestamp {secs} out of range: {e}")))
}

fn parse_id<T: serde::de::DeserializeOwned>(s: &str, label: &str) -> Result<T, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid {label} '{s}': {e}")))
}

type EventLogRow = (String, String, String, i64, String, Option<String>, i64);

fn decode_row(row: EventLogRow) -> Result<Event, SqliteStorageError> {
    let (id_str, source_str, kind, timestamp_secs, payload_str, caused_by_str, replay_int) = row;

    let id: EventId = parse_id(&id_str, "event id")?;
    let source: EventSource = parse_id(&source_str, "event source")?;
    let timestamp = from_epoch_secs(timestamp_secs)?;
    let payload: serde_json::Value = serde_json::from_str(&payload_str)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid payload json: {e}")))?;
    let caused_by: Option<EventId> = caused_by_str
        .as_deref()
        .map(|s| parse_id(s, "caused_by"))
        .transpose()?;

    Ok(Event {
        id,
        source,
        kind,
        timestamp,
        payload,
        caused_by,
        replay: replay_int != 0,
    })
}

pub struct SqliteEventLogRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteEventLogRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventLogRepo for SqliteEventLogRepo {
    async fn insert(&self, event: &Event) -> Result<(), StorageError> {
        let id = event.id.to_string();
        let source = serde_json::to_string(&event.source)
            .map_err(StorageError::Serialization)?
            .trim_matches('"')
            .to_string();
        let timestamp = to_epoch_secs(event.timestamp);
        let payload = serde_json::to_string(&event.payload).map_err(StorageError::Serialization)?;
        let caused_by = event.caused_by.map(|cid| cid.to_string());
        let replay = i64::from(event.replay);

        sqlx::query(
            "INSERT INTO event_log (id, source, kind, timestamp, payload, caused_by, replay)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&source)
        .bind(&event.kind)
        .bind(timestamp)
        .bind(&payload)
        .bind(caused_by.as_deref())
        .bind(replay)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn get(&self, id: EventId) -> Result<Option<Event>, StorageError> {
        let id_str = id.to_string();
        let row: Option<EventLogRow> = sqlx::query_as(
            "SELECT id, source, kind, timestamp, payload, caused_by, replay
             FROM event_log WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn recent(&self, limit: usize) -> Result<Vec<Event>, StorageError> {
        let rows: Vec<EventLogRow> = sqlx::query_as(
            "SELECT id, source, kind, timestamp, payload, caused_by, replay
             FROM event_log
             ORDER BY timestamp DESC
             LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }

    async fn recent_since(
        &self,
        limit: usize,
        since: Option<EventId>,
    ) -> Result<Vec<Event>, StorageError> {
        let rows: Vec<EventLogRow> = match since {
            Some(id) => {
                let id_str = id.to_string();
                sqlx::query_as(
                    "SELECT id, source, kind, timestamp, payload, caused_by, replay
                     FROM event_log
                     WHERE timestamp > (SELECT timestamp FROM event_log WHERE id = ?)
                     ORDER BY timestamp DESC
                     LIMIT ?",
                )
                .bind(&id_str)
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?
            }
            None => sqlx::query_as(
                "SELECT id, source, kind, timestamp, payload, caused_by, replay
                     FROM event_log
                     ORDER BY timestamp DESC
                     LIMIT ?",
            )
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?,
        };

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }

    async fn prune_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        let cutoff_secs = to_epoch_secs(cutoff);
        let result = sqlx::query("DELETE FROM event_log WHERE timestamp < ?")
            .bind(cutoff_secs)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{SqliteBackend, apply_migrations, connect};
    use forge_events::{Event, EventSource};
    use forge_storage::{DataProvider, EventLogRepo};

    async fn make_repo() -> SqliteEventLogRepo {
        let pool = connect(":memory:").await.unwrap();
        apply_migrations(&pool).await.unwrap();
        SqliteEventLogRepo::new(pool)
    }

    fn event_at(kind: &str, unix_secs: i64) -> Event {
        let mut ev = Event::new(EventSource::Core, kind, serde_json::Value::Null);
        ev.timestamp = OffsetDateTime::from_unix_timestamp(unix_secs).unwrap();
        ev
    }

    #[tokio::test]
    async fn recent_since_none_returns_newest_first() {
        let repo = make_repo().await;
        let events: Vec<Event> = (0..5)
            .map(|i| event_at(&format!("ev.{i}"), 1_000_000 + i))
            .collect();
        for ev in &events {
            repo.insert(ev).await.unwrap();
        }
        let result = repo.recent_since(3, None).await.unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].id, events[4].id);
        assert_eq!(result[1].id, events[3].id);
        assert_eq!(result[2].id, events[2].id);
    }

    #[tokio::test]
    async fn recent_since_anchor_returns_events_after() {
        let repo = make_repo().await;
        let events: Vec<Event> = (0..5)
            .map(|i| event_at(&format!("ev.{i}"), 1_000_000 + i))
            .collect();
        for ev in &events {
            repo.insert(ev).await.unwrap();
        }
        let anchor_id = events[2].id;
        let result = repo.recent_since(100, Some(anchor_id)).await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, events[4].id);
        assert_eq!(result[1].id, events[3].id);
    }

    #[tokio::test]
    async fn recent_since_unknown_anchor_returns_empty() {
        let repo = make_repo().await;
        for i in 0..3i64 {
            repo.insert(&event_at(&format!("ev.{i}"), 1_000_000 + i))
                .await
                .unwrap();
        }
        let ghost_id = forge_types::EventId::new();
        let result = repo.recent_since(100, Some(ghost_id)).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn recent_since_respects_limit() {
        let repo = make_repo().await;
        let events: Vec<Event> = (0..10i64)
            .map(|i| event_at(&format!("ev.{i}"), 1_000_000 + i))
            .collect();
        for ev in &events {
            repo.insert(ev).await.unwrap();
        }
        let result = repo.recent_since(4, None).await.unwrap();
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].id, events[9].id);
    }

    #[tokio::test]
    async fn recent_since_open_with_key_backend_roundtrip() {
        let backend = SqliteBackend::open_with_key(":memory:", [0xab; 32])
            .await
            .unwrap();
        let repo = backend.event_log_repo();
        let events: Vec<Event> = (0..5i64)
            .map(|i| event_at(&format!("ev.{i}"), 2_000_000 + i))
            .collect();
        for ev in &events {
            repo.insert(ev).await.unwrap();
        }
        let anchor_id = events[1].id;
        let result = repo.recent_since(100, Some(anchor_id)).await.unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].id, events[4].id);
        assert_eq!(result[1].id, events[3].id);
        assert_eq!(result[2].id, events[2].id);
    }
}
