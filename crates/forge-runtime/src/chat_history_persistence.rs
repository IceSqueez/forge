use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use forge_events::Event;
use forge_storage::{ChatHistoryRepo, SettingsRepo, chat_history_store_limit};
use forge_types::{ChatModerationAction, ChatSource, UnifiedChatRow};
use time::OffsetDateTime;

use crate::bus::EventBus;
use crate::chat_stream::{ChatRecord, ChatRecordMapper};
use crate::delivery::CHAT_HISTORY;
use crate::delivery_loss::{ConsumerLossCounters, DeliveryTier};
use crate::persist_batch::{BATCH_WRITE_TIMEOUT, BatchSink, MappedEvents, run_batched};

const PERSIST_OP_TIMEOUT: Duration = Duration::from_secs(5);
const PRUNE_EVERY_APPENDS: u64 = 256;
/// A chat row can reach this writer after a moderation action published later than it (the
/// action rides the priority lane); remembered actions are applied to such late rows.
const MODERATION_MEMORY: usize = 256;
const MODERATION_LOOKBACK: time::Duration = time::Duration::minutes(2);

/// Chat rows and moderation marks share one writer, so a mark is never applied before the rows
/// it covers are stored.
pub fn spawn_chat_history_persistence(
    bus: Arc<EventBus>,
    repo: Arc<dyn ChatHistoryRepo>,
    settings: Arc<dyn SettingsRepo>,
) {
    let mut mapper = ChatRecordMapper::default();
    let intake = MappedEvents::new(
        bus.subscribe_critical(CHAT_HISTORY),
        move |event: &Event| mapper.map(event),
    );
    let sink = ChatHistorySink {
        repo,
        settings,
        loss: bus.loss_counters(CHAT_HISTORY, DeliveryTier::Critical),
        pending: Vec::new(),
        appended_since_prune: 0,
        recent_moderation: VecDeque::new(),
    };
    tokio::spawn(run_batched(
        intake,
        sink,
        bus.batch_policy(),
        bus.flush_ticket(),
    ));
}

struct RecentModeration {
    source: ChatSource,
    action: ChatModerationAction,
    at: OffsetDateTime,
}

impl RecentModeration {
    fn mark(&self, row: &mut UnifiedChatRow) {
        match &self.action {
            ChatModerationAction::DeleteMessage { message_id } => {
                if *message_id == row.id {
                    row.moderation.deleted = true;
                }
            }
            ChatModerationAction::RemoveUser { user_name, timeout } => {
                if self.covers(row) && *user_name == row.author {
                    row.moderation.deleted = true;
                    if *timeout {
                        row.moderation.timed_out = true;
                    } else {
                        row.moderation.banned = true;
                    }
                }
            }
            ChatModerationAction::ClearChat => {
                if self.covers(row) {
                    row.moderation.deleted = true;
                }
            }
        }
    }

    fn covers(&self, row: &UnifiedChatRow) -> bool {
        row.source == self.source && row.received_at <= self.at
    }
}

struct ChatHistorySink {
    repo: Arc<dyn ChatHistoryRepo>,
    settings: Arc<dyn SettingsRepo>,
    loss: Arc<ConsumerLossCounters>,
    pending: Vec<UnifiedChatRow>,
    appended_since_prune: u64,
    recent_moderation: VecDeque<RecentModeration>,
}

impl BatchSink for ChatHistorySink {
    type Item = ChatRecord;

    /// Records not yet taken stay in `batch`, so a flush cancelled mid-commit leaves every
    /// unstored row either there or in `pending` for `abandon` to count.
    async fn flush(&mut self, batch: &mut Vec<ChatRecord>) {
        batch.reverse();
        while let Some(record) = batch.pop() {
            match record {
                ChatRecord::Row(mut row) => {
                    for moderation in &self.recent_moderation {
                        moderation.mark(&mut row);
                    }
                    self.pending.push(row);
                }
                ChatRecord::Moderation { source, action, at } => {
                    self.write_pending().await;
                    apply_moderation(self.repo.as_ref(), source, &action).await;
                    self.remember(RecentModeration { source, action, at });
                }
            }
        }
        self.write_pending().await;
        if self.appended_since_prune >= PRUNE_EVERY_APPENDS {
            self.appended_since_prune = 0;
            prune(self.repo.as_ref(), self.settings.as_ref()).await;
        }
    }

    fn abandon(&mut self, rows: u64) -> u64 {
        let rows = rows + self.pending.len() as u64;
        self.pending.clear();
        self.loss.unwritten(rows);
        rows
    }
}

impl ChatHistorySink {
    async fn write_pending(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let rows = self.pending.len() as u64;
        match tokio::time::timeout(BATCH_WRITE_TIMEOUT, self.repo.append_batch(&self.pending)).await
        {
            Ok(Ok(())) => self.appended_since_prune += rows,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, rows, "chat history batch append failed");
                self.loss.unwritten(rows);
            }
            Err(_) => {
                tracing::warn!(rows, "chat history batch append timed out");
                self.loss.unwritten(rows);
            }
        }
        self.pending.clear();
    }

    fn remember(&mut self, moderation: RecentModeration) {
        let horizon = OffsetDateTime::now_utc() - MODERATION_LOOKBACK;
        self.recent_moderation.retain(|kept| kept.at >= horizon);
        if self.recent_moderation.len() >= MODERATION_MEMORY {
            self.recent_moderation.pop_front();
        }
        self.recent_moderation.push_back(moderation);
    }
}

async fn apply_moderation(
    repo: &dyn ChatHistoryRepo,
    source: ChatSource,
    action: &ChatModerationAction,
) {
    let outcome = match action {
        ChatModerationAction::DeleteMessage { message_id } => {
            tokio::time::timeout(PERSIST_OP_TIMEOUT, repo.mark_message_deleted(message_id)).await
        }
        ChatModerationAction::RemoveUser { user_name, timeout } => {
            tokio::time::timeout(
                PERSIST_OP_TIMEOUT,
                repo.mark_user_messages_moderated(source, user_name, *timeout),
            )
            .await
        }
        ChatModerationAction::ClearChat => {
            tokio::time::timeout(PERSIST_OP_TIMEOUT, repo.clear_platform(source)).await
        }
    };
    match outcome {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "chat moderation persistence failed"),
        Err(_) => tracing::warn!("chat moderation persistence timed out"),
    }
}

async fn prune(repo: &dyn ChatHistoryRepo, settings: &dyn SettingsRepo) {
    let limit = match chat_history_store_limit(settings).await {
        Ok(limit) => limit as usize,
        Err(e) => {
            tracing::warn!(error = %e, "reading chat history store limit failed; skipping prune");
            return;
        }
    };
    match tokio::time::timeout(PERSIST_OP_TIMEOUT, repo.prune_to_limit(limit)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "chat history prune failed"),
        Err(_) => tracing::warn!("chat history prune timed out"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_storage::chat_history::MockChatHistoryRepo;
    use forge_storage::settings::MockSettingsRepo;
    use forge_storage::{DataProvider, StorageError};
    use forge_types::{ModerationMarks, UnifiedChatRow};

    use super::*;
    use crate::NullEventLogRepo;
    use crate::test_support::{Sandboxed, sandboxed_backend};

    const KEY: [u8; 32] = [0x5a; 32];
    const MINUTE: time::Duration = time::Duration::minutes(1);

    struct Rig {
        backend: Sandboxed<forge_storage_sqlite::SqliteBackend>,
        sink: ChatHistorySink,
    }

    async fn rig() -> Rig {
        let backend = sandboxed_backend(KEY).await;
        let sink = sink_over(
            backend.chat_history_repo(),
            &EventBus::new(Arc::new(NullEventLogRepo)),
        );
        Rig { backend, sink }
    }

    fn sink_over(repo: Arc<dyn ChatHistoryRepo>, bus: &EventBus) -> ChatHistorySink {
        ChatHistorySink {
            repo,
            settings: Arc::new(MockSettingsRepo::new()),
            loss: bus.loss_counters(CHAT_HISTORY, DeliveryTier::Critical),
            pending: Vec::new(),
            appended_since_prune: 0,
            recent_moderation: VecDeque::new(),
        }
    }

    fn row(id: &str, source: ChatSource, author: &str, received_at: OffsetDateTime) -> ChatRecord {
        ChatRecord::Row(UnifiedChatRow {
            id: id.to_string(),
            event_id: forge_types::EventId::new(),
            source,
            received_at,
            author: author.to_string(),
            author_color: None,
            body_segments: vec![],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        })
    }

    fn mark(source: ChatSource, action: ChatModerationAction, at: OffsetDateTime) -> ChatRecord {
        ChatRecord::Moderation { source, action, at }
    }

    fn delete(id: &str) -> ChatModerationAction {
        ChatModerationAction::DeleteMessage {
            message_id: id.to_string(),
        }
    }

    async fn flush(sink: &mut ChatHistorySink, records: Vec<ChatRecord>) {
        let mut batch = records;
        sink.flush(&mut batch).await;
    }

    async fn marks_of(rig: &Rig, id: &str) -> ModerationMarks {
        rig.backend
            .chat_history_repo()
            .list_recent(1_000)
            .await
            .unwrap()
            .into_iter()
            .find(|stored| stored.id == id)
            .map(|stored| stored.moderation)
            .unwrap_or_else(|| panic!("row {id} was not stored"))
    }

    fn deleted_only() -> ModerationMarks {
        ModerationMarks {
            deleted: true,
            ..ModerationMarks::default()
        }
    }

    #[tokio::test]
    async fn a_mark_that_follows_its_row_in_one_batch_lands_on_the_stored_row() {
        let mut rig = rig().await;
        let now = OffsetDateTime::now_utc();

        flush(
            &mut rig.sink,
            vec![
                row("m1", ChatSource::Twitch, "bob", now),
                mark(ChatSource::Twitch, delete("m1"), now),
            ],
        )
        .await;

        assert_eq!(marks_of(&rig, "m1").await, deleted_only());
    }

    #[tokio::test]
    async fn a_row_that_arrives_after_its_delete_mark_is_stored_deleted() {
        let mut rig = rig().await;
        let now = OffsetDateTime::now_utc();
        flush(
            &mut rig.sink,
            vec![mark(ChatSource::Twitch, delete("m1"), now)],
        )
        .await;

        flush(
            &mut rig.sink,
            vec![row("m1", ChatSource::Twitch, "bob", now)],
        )
        .await;

        assert_eq!(marks_of(&rig, "m1").await, deleted_only());
    }

    #[tokio::test]
    async fn a_user_removal_marks_that_users_late_rows_received_before_it_only() {
        for timeout in [true, false] {
            let mut rig = rig().await;
            let at = OffsetDateTime::now_utc();
            let removal = ChatModerationAction::RemoveUser {
                user_name: "bob".to_string(),
                timeout,
            };
            flush(&mut rig.sink, vec![mark(ChatSource::Twitch, removal, at)]).await;

            flush(
                &mut rig.sink,
                vec![
                    row("before", ChatSource::Twitch, "bob", at - MINUTE),
                    row("after", ChatSource::Twitch, "bob", at + MINUTE),
                    row("other-user", ChatSource::Twitch, "alice", at - MINUTE),
                    row("other-source", ChatSource::Kick, "bob", at - MINUTE),
                ],
            )
            .await;

            let removed = ModerationMarks {
                deleted: true,
                timed_out: timeout,
                banned: !timeout,
            };
            assert_eq!(
                [
                    marks_of(&rig, "before").await,
                    marks_of(&rig, "after").await,
                    marks_of(&rig, "other-user").await,
                    marks_of(&rig, "other-source").await,
                ],
                [
                    removed,
                    ModerationMarks::default(),
                    ModerationMarks::default(),
                    ModerationMarks::default(),
                ],
                "timeout = {timeout}"
            );
        }
    }

    #[tokio::test]
    async fn a_chat_clear_marks_late_rows_of_its_source_received_before_it_only() {
        let mut rig = rig().await;
        let at = OffsetDateTime::now_utc();
        flush(
            &mut rig.sink,
            vec![mark(
                ChatSource::YouTube,
                ChatModerationAction::ClearChat,
                at,
            )],
        )
        .await;

        flush(
            &mut rig.sink,
            vec![
                row("before", ChatSource::YouTube, "bob", at - MINUTE),
                row("after", ChatSource::YouTube, "bob", at + MINUTE),
                row("other-source", ChatSource::Twitch, "bob", at - MINUTE),
            ],
        )
        .await;

        assert_eq!(
            [
                marks_of(&rig, "before").await,
                marks_of(&rig, "after").await,
                marks_of(&rig, "other-source").await,
            ],
            [
                deleted_only(),
                ModerationMarks::default(),
                ModerationMarks::default()
            ]
        );
    }

    #[tokio::test]
    async fn the_oldest_mark_is_forgotten_once_the_memory_overflows_by_one() {
        let mut rig = rig().await;
        let now = OffsetDateTime::now_utc();
        let marks = (0..=MODERATION_MEMORY)
            .map(|i| mark(ChatSource::Twitch, delete(&format!("d{i}")), now))
            .collect();
        flush(&mut rig.sink, marks).await;

        flush(
            &mut rig.sink,
            vec![
                row("d0", ChatSource::Twitch, "bob", now),
                row("d1", ChatSource::Twitch, "bob", now),
            ],
        )
        .await;

        assert_eq!(
            [marks_of(&rig, "d0").await, marks_of(&rig, "d1").await],
            [ModerationMarks::default(), deleted_only()]
        );
    }

    #[tokio::test]
    async fn a_failed_batch_counts_every_row_it_carried_as_unwritten() {
        let mut repo = MockChatHistoryRepo::new();
        repo.expect_append_batch().returning(|_| {
            Err(StorageError::Connection {
                reason: "disk full".to_string(),
            })
        });
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sink = sink_over(Arc::new(repo), &bus);
        let now = OffsetDateTime::now_utc();

        flush(
            &mut sink,
            (0..3)
                .map(|i| row(&format!("m{i}"), ChatSource::Twitch, "bob", now))
                .collect(),
        )
        .await;

        let unwritten = bus
            .loss_report()
            .into_iter()
            .find(|entry| entry.consumer == CHAT_HISTORY)
            .map(|entry| entry.loss.unwritten);
        assert_eq!(unwritten, Some(3));
    }

    struct StuckAppends {
        appending: Arc<tokio::sync::Notify>,
    }

    #[async_trait::async_trait]
    impl ChatHistoryRepo for StuckAppends {
        async fn append(&self, _: &UnifiedChatRow) -> Result<(), StorageError> {
            self.appending.notify_one();
            std::future::pending().await
        }

        async fn list_recent(&self, _: usize) -> Result<Vec<UnifiedChatRow>, StorageError> {
            Ok(Vec::new())
        }

        async fn prune_to_limit(&self, _: usize) -> Result<u64, StorageError> {
            Ok(0)
        }

        async fn mark_message_deleted(&self, _: &str) -> Result<u64, StorageError> {
            Ok(0)
        }

        async fn mark_user_messages_moderated(
            &self,
            _: ChatSource,
            _: &str,
            _: bool,
        ) -> Result<u64, StorageError> {
            Ok(0)
        }

        async fn clear_platform(&self, _: ChatSource) -> Result<u64, StorageError> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn an_abandon_while_rows_before_a_mark_are_committing_counts_the_rows_after_it_too() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let appending = Arc::new(tokio::sync::Notify::new());
        let sink = sink_over(
            Arc::new(StuckAppends {
                appending: Arc::clone(&appending),
            }),
            &bus,
        );
        let now = OffsetDateTime::now_utc();
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        for record in [
            row("before", ChatSource::Twitch, "bob", now),
            mark(ChatSource::Twitch, delete("elsewhere"), now),
            row("after", ChatSource::Twitch, "bob", now),
        ] {
            tx.send(record).await.unwrap();
        }
        tokio::spawn(run_batched(
            rx,
            sink,
            bus.batch_policy(),
            bus.flush_ticket(),
        ));
        appending.notified().await;

        bus.shutdown();
        let given_up = tokio::time::timeout(Duration::from_secs(5), bus.abandon_flush())
            .await
            .unwrap();

        assert!(
            given_up >= 2,
            "both chat rows went unstored, {given_up} counted"
        );
    }
}
