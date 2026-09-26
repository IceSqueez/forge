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
