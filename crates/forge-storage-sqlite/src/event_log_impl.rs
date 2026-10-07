use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_storage::{EventLogRepo, StorageError};
use forge_types::EventId;
use time::OffsetDateTime;

use crate::batch::insert_rows;
use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

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

const EVENT_LOG_COLUMNS: usize = 7;

struct EncodedEvent {
    id: String,
    source: String,
    kind: String,
    timestamp: i64,
    payload: String,
    caused_by: Option<String>,
    replay: i64,
}

impl EncodedEvent {
    fn encode(event: &Event) -> Result<Self, StorageError> {
        Ok(Self {
            id: event.id.to_string(),
            source: serde_json::to_string(&event.source)
                .map_err(StorageError::Serialization)?
                .trim_matches('"')
                .to_string(),
            kind: event.kind.clone(),
            timestamp: to_epoch_secs(event.timestamp),
            payload: serde_json::to_string(&event.payload).map_err(StorageError::Serialization)?,
            caused_by: event.caused_by.map(|cid| cid.to_string()),
            replay: i64::from(event.replay),
        })
    }
}

#[derive(sqlx::FromRow)]
struct EventLogRow {
    id: String,
    source: String,
    kind: String,
    timestamp: i64,
    payload: String,
    caused_by: Option<String>,
    replay: i64,
}

fn decode_row(row: EventLogRow) -> Result<Event, SqliteStorageError> {
    let id: EventId = parse_id(&row.id, "event id")?;
    let source: EventSource = parse_id(&row.source, "event source")?;
    let timestamp = from_epoch_secs(row.timestamp)?;
    let payload: serde_json::Value = serde_json::from_str(&row.payload)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid payload json: {e}")))?;
    let caused_by: Option<EventId> = row
        .caused_by
        .as_deref()
        .map(|s| parse_id(s, "caused_by"))
        .transpose()?;
    let kind = row.kind;

    Ok(Event {
        id,
        source,
        kind,
        timestamp,
        payload,
        caused_by,
        replay: row.replay != 0,
    })
}

pub struct SqliteEventLogRepo {
    db: SqlitePools,
}

impl SqliteEventLogRepo {
    pub fn new(db: impl Into<SqlitePools>) -> Self {
        Self { db: db.into() }
    }

    pub(crate) async fn prune_chunk_before(
        &self,
        cutoff: OffsetDateTime,
        max_rows: u32,
    ) -> Result<u64, SqliteStorageError> {
        let result = sqlx::query(
            "DELETE FROM event_log WHERE rowid IN (
                 SELECT rowid FROM event_log WHERE timestamp < ? ORDER BY timestamp LIMIT ?
             )",
        )
        .bind(to_epoch_secs(cutoff))
        .bind(i64::from(max_rows))
        .execute(self.db.writer())
        .await?;
        Ok(result.rows_affected())
    }
}

#[async_trait]
impl EventLogRepo for SqliteEventLogRepo {
    async fn insert(&self, event: &Event) -> Result<(), StorageError> {
        let row = EncodedEvent::encode(event)?;
        sqlx::query(
            "INSERT INTO event_log (id, source, kind, timestamp, payload, caused_by, replay)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.source)
        .bind(&row.kind)
        .bind(row.timestamp)
        .bind(&row.payload)
        .bind(row.caused_by.as_deref())
        .bind(row.replay)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn insert_batch(&self, events: &[Arc<Event>]) -> Result<(), StorageError> {
        if events.is_empty() {
            return Ok(());
        }
        let rows = events
            .iter()
            .map(|event| EncodedEvent::encode(event))
            .collect::<Result<Vec<_>, _>>()?;

        let mut tx = self
            .db
            .writer()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        insert_rows(
            &mut tx,
            "INSERT INTO event_log (id, source, kind, timestamp, payload, caused_by, replay) ",
            EVENT_LOG_COLUMNS,
            " ON CONFLICT(id) DO NOTHING",
            &rows,
            |values, row| {
                values
                    .push_bind(row.id.as_str())
                    .push_bind(row.source.as_str())
                    .push_bind(row.kind.as_str())
                    .push_bind(row.timestamp)
                    .push_bind(row.payload.as_str())
                    .push_bind(row.caused_by.as_deref())
                    .push_bind(row.replay);
            },
        )
        .await?;
        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    async fn get(&self, id: EventId) -> Result<Option<Event>, StorageError> {
        let id_str = id.to_string();
        let row: Option<EventLogRow> = sqlx::query_as(
            "SELECT id, source, kind, timestamp, payload, caused_by, replay
             FROM event_log WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(self.db.reader())
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
        .fetch_all(self.db.reader())
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
                .fetch_all(self.db.reader())
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
            .fetch_all(self.db.reader())
            .await
            .map_err(SqliteStorageError::Sqlx)?,
        };

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{apply_migrations, connect};
    use forge_events::{Event, EventSource};
    use forge_storage::EventLogRepo;

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

    async fn surviving_timestamps(repo: &SqliteEventLogRepo) -> Vec<i64> {
        sqlx::query_scalar("SELECT timestamp FROM event_log ORDER BY timestamp")
            .fetch_all(repo.db.writer())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn prune_chunk_deletes_only_events_strictly_older_than_the_cutoff_second() {
        let repo = make_repo().await;
        let cutoff_secs = 1_700_000_000_i64;
        for offset in [-2_i64, -1, 0, 1] {
            repo.insert(&event_at("ev", cutoff_secs + offset))
                .await
                .unwrap();
        }

        let cutoff = OffsetDateTime::from_unix_timestamp(cutoff_secs).unwrap();
        let pruned = repo.prune_chunk_before(cutoff, 100).await.unwrap();

        assert_eq!(pruned, 2);
        assert_eq!(
            surviving_timestamps(&repo).await,
            vec![cutoff_secs, cutoff_secs + 1]
        );
    }

    #[tokio::test]
    async fn prune_chunk_with_a_sub_second_cutoff_keeps_a_newer_event_in_the_same_second() {
        let repo = make_repo().await;
        let second = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let mut newer = event_at("ev", 0);
        newer.timestamp = second + time::Duration::milliseconds(900);
        repo.insert(&newer).await.unwrap();

        let cutoff = second + time::Duration::milliseconds(500);
        let pruned = repo.prune_chunk_before(cutoff, 100).await.unwrap();

        assert_eq!(pruned, 0);
    }

    #[tokio::test]
    async fn prune_chunk_deletes_the_oldest_events_first_up_to_max_rows() {
        let repo = make_repo().await;
        let cutoff_secs = 1_700_000_000_i64;
        for offset in [-1_i64, -5, -3, -2, -4, 10] {
            repo.insert(&event_at("ev", cutoff_secs + offset))
                .await
                .unwrap();
        }
        let cutoff = OffsetDateTime::from_unix_timestamp(cutoff_secs).unwrap();

        let mut per_call = Vec::new();
        let mut oldest_after_call = Vec::new();
        for _ in 0..4 {
            per_call.push(repo.prune_chunk_before(cutoff, 2).await.unwrap());
            oldest_after_call.push(surviving_timestamps(&repo).await[0] - cutoff_secs);
        }

        assert_eq!(per_call, vec![2, 2, 1, 0]);
        assert_eq!(oldest_after_call, vec![-3, -1, 10, 10]);
    }
}
