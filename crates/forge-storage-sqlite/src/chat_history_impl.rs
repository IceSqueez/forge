use async_trait::async_trait;
use forge_storage::{
    AUTHORLESS_CHAT_HISTORY_RETAINED, ChatAuthorKey, ChatAuthorPage, ChatAuthorTally,
    ChatHistoryCursor, ChatHistoryRepo, StorageError,
};
use forge_types::EventId;
use forge_types::unified_chat::{
    ChatEventDetail, ChatSegment, ChatSource, ModerationMarks, UnifiedChatRow, UserBadge,
};
use time::OffsetDateTime;

use crate::batch::insert_rows;
use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

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

fn encode_source(source: ChatSource) -> Result<String, StorageError> {
    Ok(serde_json::to_string(&source)
        .map_err(StorageError::Serialization)?
        .trim_matches('"')
        .to_string())
}

const CHAT_HISTORY_COLUMNS: usize = 12;

struct EncodedChatRow<'a> {
    id: &'a str,
    event_id: String,
    source: String,
    received_at: i64,
    author: &'a str,
    author_id: Option<&'a str>,
    author_color: Option<String>,
    body_segments: String,
    badges: String,
    is_event: i64,
    event_detail: Option<String>,
    moderation: String,
}

impl<'a> EncodedChatRow<'a> {
    fn encode(row: &'a UnifiedChatRow) -> Result<Self, StorageError> {
        Ok(Self {
            id: &row.id,
            event_id: row.event_id.to_string(),
            source: encode_source(row.source)?,
            received_at: to_epoch_ms(row.received_at),
            author: &row.author,
            author_id: row.author_id.as_deref(),
            author_color: row
                .author_color
                .map(|c| serde_json::to_string(&c))
                .transpose()
                .map_err(StorageError::Serialization)?,
            body_segments: serde_json::to_string(&row.body_segments)
                .map_err(StorageError::Serialization)?,
            badges: serde_json::to_string(&row.badges).map_err(StorageError::Serialization)?,
            is_event: i64::from(row.is_event),
            event_detail: row
                .event_detail
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(StorageError::Serialization)?,
            moderation: serde_json::to_string(&row.moderation)
                .map_err(StorageError::Serialization)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct ChatHistoryRow {
    id: String,
    event_id: String,
    source: String,
    received_at: i64,
    author: String,
    author_id: Option<String>,
    author_color: Option<String>,
    body_segments: String,
    badges: String,
    is_event: i64,
    event_detail: Option<String>,
    moderation: String,
}

#[derive(sqlx::FromRow)]
struct SequencedChatHistoryRow {
    seq: i64,
    #[sqlx(flatten)]
    row: ChatHistoryRow,
}

fn decode_row(row: ChatHistoryRow) -> Result<UnifiedChatRow, SqliteStorageError> {
    let event_id: EventId = parse_id(&row.event_id, "event id")?;
    let source: ChatSource = parse_id(&row.source, "chat source")?;
    let received_at = from_epoch_ms(row.received_at)?;
    let author_color: Option<[u8; 3]> = row
        .author_color
        .as_deref()
        .map(|s| {
            serde_json::from_str(s)
                .map_err(|e| SqliteStorageError::Decode(format!("invalid author_color json: {e}")))
        })
        .transpose()?;
    let body_segments: Vec<ChatSegment> = serde_json::from_str(&row.body_segments)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid body_segments json: {e}")))?;
    let badges: Vec<UserBadge> = serde_json::from_str(&row.badges)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid badges json: {e}")))?;
    let event_detail: Option<ChatEventDetail> = row
        .event_detail
        .as_deref()
        .map(|s| {
            serde_json::from_str(s)
                .map_err(|e| SqliteStorageError::Decode(format!("invalid event_detail json: {e}")))
        })
        .transpose()?;
    let moderation: ModerationMarks = serde_json::from_str(&row.moderation)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid moderation json: {e}")))?;

    Ok(UnifiedChatRow {
        id: row.id,
        event_id,
        source,
        received_at,
        author: row.author,
        author_id: row.author_id,
        author_color,
        body_segments,
        badges,
        is_event: row.is_event != 0,
        event_detail,
        moderation,
    })
}

fn decode_rows(rows: Vec<ChatHistoryRow>) -> Result<Vec<UnifiedChatRow>, StorageError> {
    rows.into_iter()
        .map(|r| decode_row(r).map_err(StorageError::from))
        .collect()
}

async fn author_tally(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source: &str,
    author_id: &str,
) -> Result<ChatAuthorTally, StorageError> {
    let (messages, newest_ms): (i64, Option<i64>) = sqlx::query_as(
        "SELECT COUNT(*), MAX(received_at)
         FROM chat_history
         WHERE source = ? AND author_id = ? AND is_event = 0",
    )
    .bind(source)
    .bind(author_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(SqliteStorageError::Sqlx)?;
    Ok(ChatAuthorTally {
        messages: u64::try_from(messages).unwrap_or(0),
        newest_at: newest_ms.map(from_epoch_ms).transpose()?,
    })
}

pub struct SqliteChatHistoryRepo {
    db: SqlitePools,
}

impl SqliteChatHistoryRepo {
    pub fn new(db: impl Into<SqlitePools>) -> Self {
        Self { db: db.into() }
    }
}

#[async_trait]
impl ChatHistoryRepo for SqliteChatHistoryRepo {
    async fn append(&self, row: &UnifiedChatRow) -> Result<(), StorageError> {
        let row = EncodedChatRow::encode(row)?;
        sqlx::query(
            "INSERT INTO chat_history
                (id, event_id, source, received_at, author, author_id, author_color,
                 body_segments, badges, is_event, event_detail, moderation)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(row.id)
        .bind(&row.event_id)
        .bind(&row.source)
        .bind(row.received_at)
        .bind(row.author)
        .bind(row.author_id)
        .bind(row.author_color.as_deref())
        .bind(&row.body_segments)
        .bind(&row.badges)
        .bind(row.is_event)
        .bind(row.event_detail.as_deref())
        .bind(&row.moderation)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn append_batch(&self, rows: &[UnifiedChatRow]) -> Result<(), StorageError> {
        if rows.is_empty() {
            return Ok(());
        }
        let encoded = rows
            .iter()
            .map(EncodedChatRow::encode)
            .collect::<Result<Vec<_>, _>>()?;

        let mut tx = self
            .db
            .writer()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        insert_rows(
            &mut tx,
            "INSERT INTO chat_history
                (id, event_id, source, received_at, author, author_id, author_color,
                 body_segments, badges, is_event, event_detail, moderation) ",
            CHAT_HISTORY_COLUMNS,
            " ON CONFLICT(id) DO NOTHING",
            &encoded,
            |values, row| {
                values
                    .push_bind(row.id)
                    .push_bind(row.event_id.as_str())
                    .push_bind(row.source.as_str())
                    .push_bind(row.received_at)
                    .push_bind(row.author)
                    .push_bind(row.author_id)
                    .push_bind(row.author_color.as_deref())
                    .push_bind(row.body_segments.as_str())
                    .push_bind(row.badges.as_str())
                    .push_bind(row.is_event)
                    .push_bind(row.event_detail.as_deref())
                    .push_bind(row.moderation.as_str());
            },
        )
        .await?;
        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    async fn list_recent(&self, limit: usize) -> Result<Vec<UnifiedChatRow>, StorageError> {
        let rows: Vec<ChatHistoryRow> = sqlx::query_as(
            "SELECT id, event_id, source, received_at, author, author_id, author_color,
                    body_segments, badges, is_event, event_detail, moderation
             FROM chat_history
             ORDER BY seq DESC
             LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_rows(rows)
    }

    async fn author_page(
        &self,
        author: &ChatAuthorKey,
        older_than: Option<ChatHistoryCursor>,
        limit: usize,
    ) -> Result<ChatAuthorPage, StorageError> {
        let source = encode_source(author.source)?;
        let before_seq = older_than.map_or(i64::MAX, |ChatHistoryCursor(seq)| seq);
        let mut tx = self
            .db
            .reader()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        let tally = author_tally(&mut tx, &source, &author.author_id).await?;
        let mut rows: Vec<SequencedChatHistoryRow> = sqlx::query_as(
            "SELECT seq, id, event_id, source, received_at, author, author_id, author_color,
                    body_segments, badges, is_event, event_detail, moderation
             FROM chat_history
             WHERE source = ? AND author_id = ? AND is_event = 0 AND seq < ?
             ORDER BY seq DESC
             LIMIT ?",
        )
        .bind(&source)
        .bind(&author.author_id)
        .bind(before_seq)
        .bind(i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX))
        .fetch_all(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;

        let has_older = rows.len() > limit;
        rows.truncate(limit);
        let older = has_older
            .then(|| rows.last().map(|row| ChatHistoryCursor(row.seq)))
            .flatten();
        Ok(ChatAuthorPage {
            tally,
            rows: decode_rows(rows.into_iter().map(|row| row.row).collect())?,
            older,
        })
    }

    async fn author_tallies(
        &self,
        authors: &[ChatAuthorKey],
    ) -> Result<Vec<ChatAuthorTally>, StorageError> {
        if authors.is_empty() {
            return Ok(Vec::new());
        }
        let mut tx = self
            .db
            .reader()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        let mut tallies = Vec::with_capacity(authors.len());
        for author in authors {
            let source = encode_source(author.source)?;
            tallies.push(author_tally(&mut tx, &source, &author.author_id).await?);
        }
        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        Ok(tallies)
    }

    async fn apply_retention(
        &self,
        authors: &[ChatAuthorKey],
        per_author: usize,
    ) -> Result<u64, StorageError> {
        let encoded = authors
            .iter()
            .map(|author| Ok((encode_source(author.source)?, author.author_id.as_str())))
            .collect::<Result<Vec<_>, StorageError>>()?;

        let mut tx = self
            .db
            .writer()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        let mut deleted = 0;
        for (source, author_id) in &encoded {
            deleted += sqlx::query(
                "DELETE FROM chat_history
                 WHERE source = ?1 AND author_id = ?2 AND is_event = 0
                   AND seq <= (
                       SELECT seq FROM chat_history
                       WHERE source = ?1 AND author_id = ?2 AND is_event = 0
                       ORDER BY seq DESC
                       LIMIT 1 OFFSET ?3
                   )",
            )
            .bind(source)
            .bind(author_id)
            .bind(per_author as i64)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?
            .rows_affected();
        }
        deleted += sqlx::query(
            "DELETE FROM chat_history
             WHERE (author_id IS NULL OR is_event = 1)
               AND seq <= (
                   SELECT seq FROM chat_history
                   WHERE (author_id IS NULL OR is_event = 1)
                   ORDER BY seq DESC
                   LIMIT 1 OFFSET ?
               )",
        )
        .bind(AUTHORLESS_CHAT_HISTORY_RETAINED as i64)
        .execute(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?
        .rows_affected();
        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;

        Ok(deleted)
    }

    async fn mark_message_deleted(&self, platform_msg_id: &str) -> Result<u64, StorageError> {
        let result = sqlx::query(
            "UPDATE chat_history
             SET moderation = json_set(moderation, '$.deleted', json('true'))
             WHERE id = ?",
        )
        .bind(platform_msg_id)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected())
    }

    async fn mark_user_messages_moderated(
        &self,
        source: ChatSource,
        author: &str,
        timeout: bool,
    ) -> Result<u64, StorageError> {
        let source_str = encode_source(source)?;

        let result = if timeout {
            sqlx::query(
                "UPDATE chat_history
                 SET moderation = json_set(moderation, '$.timed_out', json('true'), '$.deleted', json('true'))
                 WHERE source = ? AND author = ?",
            )
            .bind(&source_str)
            .bind(author)
            .execute(self.db.writer())
            .await
        } else {
            sqlx::query(
                "UPDATE chat_history
                 SET moderation = json_set(moderation, '$.banned', json('true'), '$.deleted', json('true'))
                 WHERE source = ? AND author = ?",
            )
            .bind(&source_str)
            .bind(author)
            .execute(self.db.writer())
            .await
        }
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected())
    }

    async fn clear_platform(&self, source: ChatSource) -> Result<u64, StorageError> {
        let source_str = encode_source(source)?;

        let result = sqlx::query(
            "UPDATE chat_history
             SET moderation = json_set(moderation, '$.deleted', json('true'))
             WHERE source = ?",
        )
        .bind(&source_str)
        .execute(self.db.writer())
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

    use forge_storage::{AUTHORLESS_CHAT_HISTORY_RETAINED, ChatAuthorKey};

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
            author_id: None,
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
        let repo = make_repo().await;
        let row = UnifiedChatRow {
            id: "rich-1".to_string(),
            event_id: EventId::new(),
            source: ChatSource::YouTube,
            received_at: OffsetDateTime::from_unix_timestamp(1_700_000_123).unwrap(),
            author: "Стрімер".to_string(),
            author_id: None,
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

    fn row_full(id: &str, source: ChatSource, author: &str, secs: i64) -> UnifiedChatRow {
        UnifiedChatRow {
            source,
            author: author.to_string(),
            ..row_at(id, secs)
        }
    }

    async fn moderation_of(repo: &SqliteChatHistoryRepo, id: &str) -> ModerationMarks {
        repo.list_recent(100)
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.id == id)
            .unwrap()
            .moderation
    }

    #[tokio::test]
    async fn list_recent_orders_ties_by_insertion_seq_not_received_at() {
        let repo = make_repo().await;
        seed(&repo, &[("a", 100), ("b", 100), ("c", 100)]).await;

        let ids: Vec<String> = repo
            .list_recent(10)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, ["c", "b", "a"]);
    }

    #[tokio::test]
    async fn mark_message_deleted_flags_only_deleted_and_survives_round_trip() {
        let repo = make_repo().await;
        seed(&repo, &[("m1", 100)]).await;

        let affected = repo.mark_message_deleted("m1").await.unwrap();

        assert_eq!(affected, 1);
        assert_eq!(
            moderation_of(&repo, "m1").await,
            ModerationMarks {
                deleted: true,
                timed_out: false,
                banned: false,
            }
        );
    }

    #[tokio::test]
    async fn mark_message_deleted_unknown_id_affects_no_rows() {
        let repo = make_repo().await;
        seed(&repo, &[("m1", 100)]).await;

        assert_eq!(repo.mark_message_deleted("nope").await.unwrap(), 0);
        assert!(!moderation_of(&repo, "m1").await.deleted);
    }

    #[tokio::test]
    async fn mark_user_messages_moderated_sets_branch_flag_only_for_matching_source_and_author() {
        for (timeout, expected) in [
            (
                true,
                ModerationMarks {
                    deleted: true,
                    timed_out: true,
                    banned: false,
                },
            ),
            (
                false,
                ModerationMarks {
                    deleted: true,
                    timed_out: false,
                    banned: true,
                },
            ),
        ] {
            let repo = make_repo().await;
            repo.append(&row_full("v1", ChatSource::Twitch, "victim", 100))
                .await
                .unwrap();
            repo.append(&row_full("v2", ChatSource::Twitch, "victim", 100))
                .await
                .unwrap();
            repo.append(&row_full("by", ChatSource::Twitch, "bystander", 100))
                .await
                .unwrap();
            repo.append(&row_full("yt", ChatSource::YouTube, "victim", 100))
                .await
                .unwrap();

            let affected = repo
                .mark_user_messages_moderated(ChatSource::Twitch, "victim", timeout)
                .await
                .unwrap();

            assert_eq!(affected, 2, "timeout={timeout}");
            assert_eq!(
                moderation_of(&repo, "v1").await,
                expected,
                "timeout={timeout}"
            );
            assert_eq!(
                moderation_of(&repo, "v2").await,
                expected,
                "timeout={timeout}"
            );
            assert_eq!(
                moderation_of(&repo, "by").await,
                ModerationMarks::default(),
                "same-source other-author must be untouched, timeout={timeout}"
            );
            assert_eq!(
                moderation_of(&repo, "yt").await,
                ModerationMarks::default(),
                "same-author other-source must be untouched, timeout={timeout}"
            );
        }
    }

    #[tokio::test]
    async fn clear_platform_marks_only_that_source_deleted() {
        let repo = make_repo().await;
        repo.append(&row_full("t1", ChatSource::Twitch, "a", 100))
            .await
            .unwrap();
        repo.append(&row_full("t2", ChatSource::Twitch, "b", 100))
            .await
            .unwrap();
        repo.append(&row_full("y1", ChatSource::YouTube, "c", 100))
            .await
            .unwrap();

        let affected = repo.clear_platform(ChatSource::Twitch).await.unwrap();

        assert_eq!(affected, 2);
        assert_eq!(
            moderation_of(&repo, "t1").await,
            ModerationMarks {
                deleted: true,
                timed_out: false,
                banned: false,
            }
        );
        assert!(moderation_of(&repo, "t2").await.deleted);
        assert_eq!(moderation_of(&repo, "y1").await, ModerationMarks::default());
    }

    const EVERY_ROW: usize = 100_000;

    fn authored(
        id: &str,
        source: ChatSource,
        author_id: Option<&str>,
        is_event: bool,
    ) -> UnifiedChatRow {
        UnifiedChatRow {
            source,
            author_id: author_id.map(str::to_owned),
            is_event,
            ..row_at(id, 100)
        }
    }

    fn key(source: ChatSource, author_id: &str) -> ChatAuthorKey {
        ChatAuthorKey {
            source,
            author_id: author_id.to_owned(),
        }
    }

    async fn append_all(repo: &SqliteChatHistoryRepo, rows: &[UnifiedChatRow]) {
        for row in rows {
            repo.append(row).await.unwrap();
        }
    }

    async fn stored_ids(repo: &SqliteChatHistoryRepo) -> Vec<String> {
        let mut ids: Vec<String> = repo
            .list_recent(EVERY_ROW)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        ids.sort();
        ids
    }

    fn messages_of(prefix: &str, count: usize) -> Vec<UnifiedChatRow> {
        (0..count)
            .map(|i| {
                authored(
                    &format!("{prefix}{i}"),
                    ChatSource::Twitch,
                    Some(prefix),
                    false,
                )
            })
            .collect()
    }

    async fn ids_by_author(
        repo: &SqliteChatHistoryRepo,
        author: &ChatAuthorKey,
        limit: usize,
    ) -> Vec<String> {
        repo.author_page(author, None, limit)
            .await
            .unwrap()
            .rows
            .into_iter()
            .map(|r| r.id)
            .collect()
    }

    #[tokio::test]
    async fn list_recent_messages_by_author_returns_newest_by_insertion_order_capped_at_limit() {
        let repo = make_repo().await;
        for (id, secs) in [("m1", 400), ("m2", 300), ("m3", 200), ("m4", 100)] {
            repo.append(&UnifiedChatRow {
                received_at: OffsetDateTime::from_unix_timestamp(secs).unwrap(),
                ..authored(id, ChatSource::Twitch, Some("42"), false)
            })
            .await
            .unwrap();
        }

        let ids = ids_by_author(&repo, &key(ChatSource::Twitch, "42"), 3).await;

        assert_eq!(ids, ["m4", "m3", "m2"]);
    }

    #[tokio::test]
    async fn list_recent_messages_by_author_skips_events_other_authors_and_other_sources() {
        let repo = make_repo().await;
        append_all(
            &repo,
            &[
                authored("mine", ChatSource::Twitch, Some("42"), false),
                authored("my_event", ChatSource::Twitch, Some("42"), true),
                authored("other_author", ChatSource::Twitch, Some("43"), false),
                authored(
                    "same_id_other_source",
                    ChatSource::YouTube,
                    Some("42"),
                    false,
                ),
                authored("authorless", ChatSource::Twitch, None, false),
            ],
        )
        .await;

        let ids = ids_by_author(&repo, &key(ChatSource::Twitch, "42"), 10).await;

        assert_eq!(ids, ["mine"]);
    }

    #[tokio::test]
    async fn apply_retention_keeps_at_most_the_newest_per_author_rows_around_the_bound() {
        for (stored, expected_kept, expected_deleted) in [
            (2, vec!["a0", "a1"], 0),
            (3, vec!["a0", "a1", "a2"], 0),
            (4, vec!["a1", "a2", "a3"], 1),
            (6, vec!["a3", "a4", "a5"], 3),
        ] {
            let repo = make_repo().await;
            append_all(&repo, &messages_of("a", stored)).await;

            let deleted = repo
                .apply_retention(&[key(ChatSource::Twitch, "a")], 3)
                .await
                .unwrap();

            assert_eq!(stored_ids(&repo).await, expected_kept, "stored={stored}");
            assert_eq!(deleted, expected_deleted, "stored={stored}");
        }
    }

    #[tokio::test]
    async fn apply_retention_leaves_unlisted_authors_and_the_same_id_on_another_source_untouched() {
        let repo = make_repo().await;
        append_all(&repo, &messages_of("a", 3)).await;
        append_all(&repo, &messages_of("b", 3)).await;
        append_all(
            &repo,
            &[
                authored("yt0", ChatSource::YouTube, Some("a"), false),
                authored("yt1", ChatSource::YouTube, Some("a"), false),
            ],
        )
        .await;

        repo.apply_retention(&[key(ChatSource::Twitch, "a")], 1)
            .await
            .unwrap();

        assert_eq!(
            stored_ids(&repo).await,
            ["a2", "b0", "b1", "b2", "yt0", "yt1"]
        );
    }

    #[tokio::test]
    async fn apply_retention_with_no_listed_authors_keeps_every_authored_message() {
        let repo = make_repo().await;
        append_all(&repo, &messages_of("a", 4)).await;

        let deleted = repo.apply_retention(&[], 1).await.unwrap();

        assert_eq!((deleted, stored_ids(&repo).await.len()), (0, 4));
    }

    #[tokio::test]
    async fn apply_retention_spares_event_rows_of_a_listed_author_from_the_per_author_bound() {
        let repo = make_repo().await;
        append_all(
            &repo,
            &[
                authored("event_old", ChatSource::Twitch, Some("a"), true),
                authored("msg_old", ChatSource::Twitch, Some("a"), false),
                authored("msg_new", ChatSource::Twitch, Some("a"), false),
            ],
        )
        .await;

        repo.apply_retention(&[key(ChatSource::Twitch, "a")], 1)
            .await
            .unwrap();

        assert_eq!(stored_ids(&repo).await, ["event_old", "msg_new"]);
    }

    fn authorless_rows(count: usize) -> Vec<UnifiedChatRow> {
        (0..count)
            .map(|i| authored(&format!("n{i:05}"), ChatSource::Twitch, None, false))
            .collect()
    }

    #[tokio::test]
    async fn apply_retention_keeps_exactly_the_authorless_bound_of_newest_rows() {
        for (extra, expected_deleted) in [(0, 0), (1, 1)] {
            let repo = make_repo().await;
            let rows = authorless_rows(AUTHORLESS_CHAT_HISTORY_RETAINED + extra);
            repo.append_batch(&rows).await.unwrap();

            let deleted = repo.apply_retention(&[], 1).await.unwrap();

            let ids = stored_ids(&repo).await;
            assert_eq!(deleted, expected_deleted, "extra={extra}");
            assert_eq!(ids.len(), AUTHORLESS_CHAT_HISTORY_RETAINED, "extra={extra}");
            assert_eq!(ids.first(), Some(&rows[extra].id), "extra={extra}");
        }
    }

    #[tokio::test]
    async fn apply_retention_counts_authored_events_against_the_authorless_bound_but_not_messages()
    {
        let repo = make_repo().await;
        append_all(
            &repo,
            &[
                authored("authored_event", ChatSource::Twitch, Some("a"), true),
                authored("authored_message", ChatSource::Twitch, Some("a"), false),
            ],
        )
        .await;
        repo.append_batch(&authorless_rows(AUTHORLESS_CHAT_HISTORY_RETAINED))
            .await
            .unwrap();

        repo.apply_retention(&[], 1).await.unwrap();

        let ids = stored_ids(&repo).await;
        assert_eq!(
            (
                ids.contains(&"authored_event".to_owned()),
                ids.contains(&"authored_message".to_owned()),
            ),
            (false, true)
        );
    }
}
