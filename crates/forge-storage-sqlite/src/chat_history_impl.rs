use async_trait::async_trait;
use forge_storage::{ChatHistoryRepo, StorageError};
use forge_types::EventId;
use forge_types::unified_chat::{
    ChatEventDetail, ChatSegment, ChatSource, ModerationMarks, UnifiedChatRow, UserBadge,
};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    (dt.unix_timestamp_nanos() / 1_000_000) as i64
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch {ms}: {e}")))
}

fn parse_id<T: serde::de::DeserializeOwned>(s: &str, label: &str) -> Result<T, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid {label} '{s}': {e}")))
}

type ChatHistoryRow = (
    String,
    String,
    String,
    i64,
    String,
    Option<String>,
    String,
    String,
    i64,
    Option<String>,
    String,
);

fn decode_row(row: ChatHistoryRow) -> Result<UnifiedChatRow, SqliteStorageError> {
    let (
        id,
        event_id_str,
        source_str,
        received_at_ms,
        author,
        author_color_str,
        body_segments_str,
        badges_str,
        is_event_int,
        event_detail_str,
        moderation_str,
    ) = row;

    let event_id: EventId = parse_id(&event_id_str, "event id")?;
    let source: ChatSource = parse_id(&source_str, "chat source")?;
    let received_at = from_epoch_ms(received_at_ms)?;
    let author_color: Option<[u8; 3]> = author_color_str
        .as_deref()
        .map(|s| {
            serde_json::from_str(s)
                .map_err(|e| SqliteStorageError::Decode(format!("invalid author_color json: {e}")))
        })
        .transpose()?;
    let body_segments: Vec<ChatSegment> = serde_json::from_str(&body_segments_str)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid body_segments json: {e}")))?;
    let badges: Vec<UserBadge> = serde_json::from_str(&badges_str)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid badges json: {e}")))?;
    let event_detail: Option<ChatEventDetail> = event_detail_str
        .as_deref()
        .map(|s| {
            serde_json::from_str(s)
                .map_err(|e| SqliteStorageError::Decode(format!("invalid event_detail json: {e}")))
        })
        .transpose()?;
    let moderation: ModerationMarks = serde_json::from_str(&moderation_str)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid moderation json: {e}")))?;

    Ok(UnifiedChatRow {
        id,
        event_id,
        source,
        received_at,
        author,
        author_color,
        body_segments,
        badges,
        is_event: is_event_int != 0,
        event_detail,
        moderation,
    })
}

pub struct SqliteChatHistoryRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteChatHistoryRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ChatHistoryRepo for SqliteChatHistoryRepo {
    async fn append(&self, row: &UnifiedChatRow) -> Result<(), StorageError> {
        let event_id_str = row.event_id.to_string();
        let source_str = serde_json::to_string(&row.source)
            .map_err(StorageError::Serialization)?
            .trim_matches('"')
            .to_string();
        let received_at_ms = to_epoch_ms(row.received_at);
        let author_color_str = row
            .author_color
            .map(|c| serde_json::to_string(&c))
            .transpose()
            .map_err(StorageError::Serialization)?;
        let body_segments_str =
            serde_json::to_string(&row.body_segments).map_err(StorageError::Serialization)?;
        let badges_str = serde_json::to_string(&row.badges).map_err(StorageError::Serialization)?;
        let event_detail_str = row
            .event_detail
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(StorageError::Serialization)?;
        let moderation_str =
            serde_json::to_string(&row.moderation).map_err(StorageError::Serialization)?;

        sqlx::query(
            "INSERT INTO chat_history
                (id, event_id, source, received_at, author, author_color,
                 body_segments, badges, is_event, event_detail, moderation)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&event_id_str)
        .bind(&source_str)
        .bind(received_at_ms)
        .bind(&row.author)
        .bind(author_color_str.as_deref())
        .bind(&body_segments_str)
        .bind(&badges_str)
        .bind(i64::from(row.is_event))
        .bind(event_detail_str.as_deref())
        .bind(&moderation_str)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn list_recent(&self, limit: usize) -> Result<Vec<UnifiedChatRow>, StorageError> {
        let rows: Vec<ChatHistoryRow> = sqlx::query_as(
            "SELECT id, event_id, source, received_at, author, author_color,
                    body_segments, badges, is_event, event_detail, moderation
             FROM chat_history
             ORDER BY received_at DESC
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

    async fn prune_to_limit(&self, max_rows: usize) -> Result<u64, StorageError> {
        let result = sqlx::query(
            "DELETE FROM chat_history
             WHERE id NOT IN (
                 SELECT id FROM chat_history ORDER BY received_at DESC LIMIT ?
             )",
        )
        .bind(max_rows as i64)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_storage::ChatHistoryRepo;
    use forge_types::EventId;
    use forge_types::unified_chat::{
        ChatEventDetail, ChatSegment, ChatSource, ModerationMarks, UnifiedChatRow, UserBadge,
    };
    use time::OffsetDateTime;

    use super::SqliteChatHistoryRepo;
    use crate::{apply_migrations, connect};

    async fn make_repo() -> SqliteChatHistoryRepo {
        let pool = connect(":memory:").await.unwrap();
        apply_migrations(&pool).await.unwrap();
        SqliteChatHistoryRepo::new(pool)
    }

    fn row_at(id: &str, unix_secs: i64) -> UnifiedChatRow {
        UnifiedChatRow {
            id: id.to_string(),
            event_id: EventId::new(),
            source: ChatSource::Twitch,
            received_at: OffsetDateTime::from_unix_timestamp(unix_secs).unwrap(),
            author: "user".to_string(),
            author_color: None,
            body_segments: vec![],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        }
    }

    async fn seed(repo: &SqliteChatHistoryRepo, ids_and_secs: &[(&str, i64)]) {
        for (id, secs) in ids_and_secs {
            repo.append(&row_at(id, *secs)).await.unwrap();
        }
    }

    #[tokio::test]
    async fn append_then_list_recent_preserves_all_rich_fields() {
        // Every rich field goes through a distinct encoding path: source as a
        // lowercase enum string, received_at as epoch-ms, author_color/segments/
        // badges/event_detail/moderation as JSON columns. Comparing the decoded
        // row against the original proves each survives the round-trip. received_at
        // is whole seconds so the ms-truncating epoch encoding is lossless here.
        let repo = make_repo().await;
        let row = UnifiedChatRow {
            id: "rich-1".to_string(),
            event_id: EventId::new(),
            source: ChatSource::YouTube,
            received_at: OffsetDateTime::from_unix_timestamp(1_700_000_123).unwrap(),
            author: "Стрімер".to_string(),
            author_color: Some([0x12, 0xAB, 0xFF]),
            body_segments: vec![
                ChatSegment::Text {
                    text: "gg ".to_string(),
                },
                ChatSegment::Emote {
                    id: "42".to_string(),
                    name: "KEKW".to_string(),
                },
                ChatSegment::Mention {
                    username: "mod".to_string(),
                },
            ],
            badges: vec![UserBadge::Moderator, UserBadge::Subscriber { months: 12 }],
            is_event: true,
            event_detail: Some(ChatEventDetail::SuperChat {
                amount_micros: 5_000_000,
                currency: "USD".to_string(),
                message: Some("thx".to_string()),
            }),
            moderation: ModerationMarks {
                deleted: true,
                timed_out: false,
                banned: true,
            },
        };

        repo.append(&row).await.unwrap();
        let got = repo.list_recent(10).await.unwrap();

        assert_eq!(got.len(), 1);
        assert_eq!(
            serde_json::to_value(&got[0]).unwrap(),
            serde_json::to_value(&row).unwrap()
        );
    }

    #[tokio::test]
    async fn list_recent_returns_newest_first_and_caps_at_limit() {
        let repo = make_repo().await;
        seed(
            &repo,
            &[("a", 100), ("b", 200), ("c", 300), ("d", 400), ("e", 500)],
        )
        .await;

        let got = repo.list_recent(3).await.unwrap();

        let ids: Vec<&str> = got.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["e", "d", "c"]);
    }

    #[tokio::test]
    async fn prune_to_limit_keeps_newest_rows_and_reports_deleted_count() {
        let repo = make_repo().await;
        seed(
            &repo,
            &[("a", 100), ("b", 200), ("c", 300), ("d", 400), ("e", 500)],
        )
        .await;

        let deleted = repo.prune_to_limit(2).await.unwrap();

        assert_eq!(deleted, 3);
        let remaining: Vec<String> = repo
            .list_recent(10)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(remaining, ["e", "d"]);
    }

    #[tokio::test]
    async fn prune_to_limit_is_noop_when_limit_meets_or_exceeds_row_count() {
        let repo = make_repo().await;
        seed(&repo, &[("a", 100), ("b", 200), ("c", 300)]).await;

        assert_eq!(repo.prune_to_limit(3).await.unwrap(), 0);
        assert_eq!(repo.prune_to_limit(10).await.unwrap(), 0);
        assert_eq!(repo.list_recent(10).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn prune_to_limit_zero_deletes_all_rows() {
        let repo = make_repo().await;
        seed(&repo, &[("a", 100), ("b", 200)]).await;

        assert_eq!(repo.prune_to_limit(0).await.unwrap(), 2);
        assert!(repo.list_recent(10).await.unwrap().is_empty());
    }
}
