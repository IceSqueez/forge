use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use forge_events::Event;
use forge_storage::{
    ChatAuthorKey, ChatHistoryRepo, DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT, SettingsRepo,
    chat_history_per_viewer_limit, clamp_chat_history_per_viewer_limit,
};
use forge_types::{ChatModerationAction, ChatSource, UnifiedChatRow};
use time::OffsetDateTime;
use tokio::sync::watch;

use crate::bus::EventBus;
use crate::chat_stream::{ChatRecord, ChatRecordMapper};
use crate::delivery::CHAT_HISTORY;
use crate::delivery_loss::{ConsumerLossCounters, DeliveryTier};
use crate::persist_batch::{BATCH_WRITE_TIMEOUT, BatchSink, MappedEvents, run_batched};

const PERSIST_OP_TIMEOUT: Duration = Duration::from_secs(5);
const MODERATION_MEMORY: usize = 256;
const MODERATION_LOOKBACK: time::Duration = time::Duration::minutes(2);

#[derive(Clone)]
pub struct ChatHistoryRetentionHandle {
    per_viewer: Arc<watch::Sender<Option<usize>>>,
}

impl ChatHistoryRetentionHandle {
    pub fn set_per_viewer_limit(&self, limit: u32) {
        self.per_viewer
            .send_replace(Some(clamp_chat_history_per_viewer_limit(limit) as usize));
    }

    async fn adopt_stored_limit(&self, settings: &dyn SettingsRepo) {
        let stored = stored_per_viewer_limit(settings).await;
        self.per_viewer.send_if_modified(|current| {
            if current.is_some() {
                return false;
            }
            *current = Some(stored);
            true
        });
    }
}

pub fn spawn_chat_history_persistence(
    bus: Arc<EventBus>,
    repo: Arc<dyn ChatHistoryRepo>,
    settings: Arc<dyn SettingsRepo>,
) -> ChatHistoryRetentionHandle {
    let (per_viewer, per_viewer_limit) = watch::channel(None);
    let handle = ChatHistoryRetentionHandle {
        per_viewer: Arc::new(per_viewer),
    };
    let mut mapper = ChatRecordMapper::default();
    let intake = MappedEvents::new(
        bus.subscribe_critical(CHAT_HISTORY),
        move |event: &Event| mapper.map(event),
    );
    let sink = ChatHistorySink {
        repo,
        per_viewer_limit,
        loss: bus.loss_counters(CHAT_HISTORY, DeliveryTier::Critical),
        pending: Vec::new(),
        recent_moderation: VecDeque::new(),
    };
    let policy = bus.batch_policy();
    let ticket = bus.flush_ticket();
    let loader = handle.clone();
    tokio::spawn(async move {
        loader.adopt_stored_limit(settings.as_ref()).await;
        run_batched(intake, sink, policy, ticket).await;
    });
    handle
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
    per_viewer_limit: watch::Receiver<Option<usize>>,
    loss: Arc<ConsumerLossCounters>,
    pending: Vec<UnifiedChatRow>,
    recent_moderation: VecDeque<RecentModeration>,
}

impl BatchSink for ChatHistorySink {
    type Item = ChatRecord;

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
            Ok(Ok(())) => self.retain_written_authors().await,
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

    async fn retain_written_authors(&self) {
        let authors: Vec<ChatAuthorKey> = self
            .pending
            .iter()
            .filter_map(|row| {
                row.author_id.as_ref().map(|author_id| ChatAuthorKey {
                    source: row.source,
                    author_id: author_id.clone(),
                })
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let per_viewer = (*self.per_viewer_limit.borrow())
            .unwrap_or(DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT as usize);
        match tokio::time::timeout(
            PERSIST_OP_TIMEOUT,
            self.repo.apply_retention(&authors, per_viewer),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => tracing::warn!(error = %e, "chat history retention failed"),
            Err(_) => tracing::warn!("chat history retention timed out"),
        }
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

async fn stored_per_viewer_limit(settings: &dyn SettingsRepo) -> usize {
    match tokio::time::timeout(PERSIST_OP_TIMEOUT, chat_history_per_viewer_limit(settings)).await {
        Ok(Ok(limit)) => limit as usize,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "reading chat history per-viewer limit failed; using the default");
            DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT as usize
        }
        Err(_) => {
            tracing::warn!("reading chat history per-viewer limit timed out; using the default");
            DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT as usize
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_storage::chat_history::MockChatHistoryRepo;
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
            per_viewer_limit: watch::channel(None).1,
            loss: bus.loss_counters(CHAT_HISTORY, DeliveryTier::Critical),
            pending: Vec::new(),
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
            author_id: None,
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

    fn by(id: &str, source: ChatSource, viewer: Option<&str>) -> ChatRecord {
        let ChatRecord::Row(mut written) = row(id, source, "name", OffsetDateTime::now_utc())
        else {
            unreachable!("row always builds a chat row");
        };
        written.author_id = viewer.map(str::to_string);
        ChatRecord::Row(written)
    }

    fn key(source: ChatSource, viewer: &str) -> ChatAuthorKey {
        ChatAuthorKey {
            source,
            author_id: viewer.to_string(),
        }
    }

    async fn retained_authors(batch: Vec<ChatRecord>) -> Vec<ChatAuthorKey> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut repo = MockChatHistoryRepo::new();
        repo.expect_append_batch().returning(|_| Ok(()));
        repo.expect_apply_retention()
            .times(1)
            .returning(move |authors, _| {
                tx.send(authors.to_vec()).unwrap();
                Ok(0)
            });
        let mut sink = sink_over(Arc::new(repo), &EventBus::new(Arc::new(NullEventLogRepo)));

        flush(&mut sink, batch).await;

        rx.try_recv().unwrap()
    }

    #[tokio::test]
    async fn retention_covers_each_distinct_viewer_of_the_written_batch_once() {
        let authors = retained_authors(vec![
            by("a", ChatSource::Twitch, Some("u1")),
            by("b", ChatSource::Twitch, Some("u1")),
            by("c", ChatSource::Kick, Some("u1")),
            by("d", ChatSource::YouTube, Some("u2")),
            by("e", ChatSource::Twitch, None),
        ])
        .await;

        assert_eq!(
            (authors.len(), authors.into_iter().collect::<HashSet<_>>()),
            (
                3,
                HashSet::from([
                    key(ChatSource::Twitch, "u1"),
                    key(ChatSource::Kick, "u1"),
                    key(ChatSource::YouTube, "u2"),
                ])
            )
        );
    }

    #[tokio::test]
    async fn a_batch_without_viewer_ids_still_runs_retention_for_the_authorless_bound() {
        let authors = retained_authors(vec![
            by("a", ChatSource::Twitch, None),
            by("b", ChatSource::Kick, None),
        ])
        .await;

        assert!(authors.is_empty(), "{} viewer keys", authors.len());
    }

    #[derive(Debug, Clone, Copy)]
    enum Outcome {
        Succeeds,
        Fails,
        Stalls,
    }

    async fn settle(outcome: Outcome) -> Result<(), StorageError> {
        match outcome {
            Outcome::Succeeds => Ok(()),
            Outcome::Fails => Err(StorageError::Connection {
                reason: "disk full".to_string(),
            }),
            Outcome::Stalls => std::future::pending().await,
        }
    }

    struct ScriptedRepo {
        append: Outcome,
        first_retention: Outcome,
        appended: std::sync::Mutex<Vec<Vec<String>>>,
        retentions: std::sync::atomic::AtomicUsize,
    }

    impl ScriptedRepo {
        fn new(append: Outcome, first_retention: Outcome) -> Arc<Self> {
            Arc::new(Self {
                append,
                first_retention,
                appended: std::sync::Mutex::new(Vec::new()),
                retentions: std::sync::atomic::AtomicUsize::new(0),
            })
        }

        fn retention_runs(&self) -> usize {
            self.retentions.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl ChatHistoryRepo for ScriptedRepo {
        async fn append(&self, row: &UnifiedChatRow) -> Result<(), StorageError> {
            self.append_batch(std::slice::from_ref(row)).await
        }

        async fn append_batch(&self, rows: &[UnifiedChatRow]) -> Result<(), StorageError> {
            self.appended
                .lock()
                .unwrap()
                .push(rows.iter().map(|row| row.id.clone()).collect());
            settle(self.append).await
        }

        async fn list_recent(&self, _: usize) -> Result<Vec<UnifiedChatRow>, StorageError> {
            Ok(Vec::new())
        }

        async fn list_recent_messages_by_author(
            &self,
            _: &ChatAuthorKey,
            _: usize,
        ) -> Result<Vec<UnifiedChatRow>, StorageError> {
            Ok(Vec::new())
        }

        async fn apply_retention(
            &self,
            _: &[ChatAuthorKey],
            _: usize,
        ) -> Result<u64, StorageError> {
            let nth = self
                .retentions
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let outcome = if nth == 0 {
                self.first_retention
            } else {
                Outcome::Succeeds
            };
            settle(outcome).await.map(|()| 0)
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

    fn unwritten(bus: &EventBus) -> Option<u64> {
        bus.loss_report()
            .into_iter()
            .find(|entry| entry.consumer == CHAT_HISTORY)
            .map(|entry| entry.loss.unwritten)
    }

    #[tokio::test(start_paused = true)]
    async fn no_retention_runs_after_a_failed_or_stalled_append() {
        for append in [Outcome::Fails, Outcome::Stalls] {
            let repo = ScriptedRepo::new(append, Outcome::Succeeds);
            let mut sink = sink_over(repo.clone(), &EventBus::new(Arc::new(NullEventLogRepo)));

            flush(&mut sink, vec![by("m", ChatSource::Twitch, Some("u1"))]).await;

            assert_eq!(repo.retention_runs(), 0, "append {append:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_or_stalled_retention_is_skipped_and_the_next_batch_is_still_written() {
        for retention in [Outcome::Fails, Outcome::Stalls] {
            let bus = EventBus::new(Arc::new(NullEventLogRepo));
            let repo = ScriptedRepo::new(Outcome::Succeeds, retention);
            let mut sink = sink_over(repo.clone(), &bus);

            flush(&mut sink, vec![by("first", ChatSource::Twitch, Some("u1"))]).await;
            flush(
                &mut sink,
                vec![by("second", ChatSource::Twitch, Some("u1"))],
            )
            .await;

            assert_eq!(
                (
                    repo.appended.lock().unwrap().clone(),
                    repo.retention_runs(),
                    unwritten(&bus).unwrap_or(0),
                ),
                (
                    vec![vec!["first".to_string()], vec!["second".to_string()]],
                    2,
                    0,
                ),
                "retention {retention:?}"
            );
        }
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

        async fn list_recent_messages_by_author(
            &self,
            _: &forge_storage::ChatAuthorKey,
            _: usize,
        ) -> Result<Vec<UnifiedChatRow>, StorageError> {
            Ok(Vec::new())
        }

        async fn apply_retention(
            &self,
            _: &[forge_storage::ChatAuthorKey],
            _: usize,
        ) -> Result<u64, StorageError> {
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
