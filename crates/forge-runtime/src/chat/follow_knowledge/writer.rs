use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;
use std::time::Duration;

use forge_events::Event;
use forge_storage::{
    EventLogRepo, FollowKnowledgeEntry, FollowKnowledgeKey, FollowKnowledgeRepo, KnownFollow,
};
use forge_types::PlatformId;
use time::OffsetDateTime;
use tokio::sync::{mpsc, oneshot, watch};

use super::policy::{FollowTranslation, is_fresh};
use crate::delivery::CriticalSubscription;

pub(super) const STORE_OP_TIMEOUT: Duration = Duration::from_secs(5);
const STARTUP_OP_TIMEOUT: Duration = Duration::from_secs(30);
const SEED_SCAN_LIMIT: usize = 50_000;

pub(super) enum WriteCommand {
    Record {
        entry: FollowKnowledgeEntry,
        reply: oneshot::Sender<KnownFollow>,
    },
    Clear {
        platform: PlatformId,
        reply: oneshot::Sender<()>,
    },
}

pub(super) struct FollowWriter {
    pub(super) repo: Arc<dyn FollowKnowledgeRepo>,
    pub(super) translation: FollowTranslation,
    pub(super) revision: Arc<watch::Sender<u64>>,
}

impl FollowWriter {
    pub(super) async fn run(
        self,
        event_log: Arc<dyn EventLogRepo>,
        mut intake: CriticalSubscription,
        mut commands: mpsc::Receiver<WriteCommand>,
    ) {
        self.prune().await;
        self.seed(event_log.as_ref()).await;
        drop(event_log);
        let mut intake_open = true;
        let mut commands_open = true;
        while intake_open || commands_open {
            tokio::select! {
                event = intake.recv(), if intake_open => match event {
                    Some(event) => self.take(&event).await,
                    None => intake_open = false,
                },
                command = commands.recv(), if commands_open => match command {
                    Some(command) => self.apply(command).await,
                    None => commands_open = false,
                },
            }
        }
    }

    async fn prune(&self) {
        let now = OffsetDateTime::now_utc();
        match tokio::time::timeout(STARTUP_OP_TIMEOUT, self.repo.prune_expired(now)).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => tracing::warn!(error = %e, "follow knowledge prune failed"),
            Err(_) => tracing::warn!("follow knowledge prune timed out"),
        }
    }

    async fn seed(&self, event_log: &dyn EventLogRepo) {
        match tokio::time::timeout(STORE_OP_TIMEOUT, self.repo.is_seeded()).await {
            Ok(Ok(true)) => return,
            Ok(Ok(false)) => {}
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "follow knowledge seed check failed; seeding skipped");
                return;
            }
            Err(_) => {
                tracing::warn!("follow knowledge seed check timed out; seeding skipped");
                return;
            }
        }
        let events =
            match tokio::time::timeout(STARTUP_OP_TIMEOUT, event_log.recent(SEED_SCAN_LIMIT)).await
            {
                Ok(Ok(events)) => events,
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "event log read for follow seeding failed");
                    return;
                }
                Err(_) => {
                    tracing::warn!("event log read for follow seeding timed out");
                    return;
                }
            };
        let entries = self.newest_fresh_entries(&events);
        match tokio::time::timeout(STARTUP_OP_TIMEOUT, self.repo.seed(&entries)).await {
            Ok(Ok(())) => {
                if !entries.is_empty() {
                    self.advance();
                }
            }
            Ok(Err(e)) => tracing::warn!(error = %e, "follow knowledge seeding failed"),
            Err(_) => tracing::warn!("follow knowledge seeding timed out"),
        }
    }

    fn newest_fresh_entries(&self, events: &[Event]) -> Vec<FollowKnowledgeEntry> {
        let now = OffsetDateTime::now_utc();
        let mut newest: HashMap<FollowKnowledgeKey, FollowKnowledgeEntry> = HashMap::new();
        for entry in events
            .iter()
            .filter_map(|event| self.translation.entry_of(event))
            .filter(|entry| is_fresh(entry, now))
        {
            match newest.entry(entry.key.clone()) {
                Entry::Occupied(mut kept) => {
                    if entry.observed_at > kept.get().observed_at {
                        kept.insert(entry);
                    }
                }
                Entry::Vacant(slot) => {
                    slot.insert(entry);
                }
            }
        }
        newest.into_values().collect()
    }

    async fn take(&self, event: &Event) {
        if let Some(entry) = self.translation.entry_of(event) {
            self.record(entry).await;
        }
    }

    async fn apply(&self, command: WriteCommand) {
        match command {
            WriteCommand::Record { entry, reply } => {
                let winner = self.record(entry).await;
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the requester may stop waiting for the reply"
                )]
                let _ = reply.send(winner);
            }
            WriteCommand::Clear { platform, reply } => {
                self.clear(platform).await;
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the requester may stop waiting for the reply"
                )]
                let _ = reply.send(());
            }
        }
    }

    async fn record(&self, entry: FollowKnowledgeEntry) -> KnownFollow {
        let now = OffsetDateTime::now_utc();
        if !is_fresh(&entry, now) {
            return entry.follow;
        }
        match tokio::time::timeout(STORE_OP_TIMEOUT, self.repo.get(&entry.key)).await {
            Ok(Ok(Some(stored)))
                if stored.observed_at > entry.observed_at && is_fresh(&stored, now) =>
            {
                return stored.follow;
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                tracing::warn!(error = %e, platform = ?entry.key.platform, "follow knowledge read before write failed");
                return entry.follow;
            }
            Err(_) => {
                tracing::warn!(platform = ?entry.key.platform, "follow knowledge read before write timed out");
                return entry.follow;
            }
        }
        match tokio::time::timeout(STORE_OP_TIMEOUT, self.repo.upsert(&entry)).await {
            Ok(Ok(())) => self.advance(),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, platform = ?entry.key.platform, "follow knowledge write failed");
            }
            Err(_) => {
                tracing::warn!(platform = ?entry.key.platform, "follow knowledge write timed out");
            }
        }
        entry.follow
    }

    async fn clear(&self, platform: PlatformId) {
        match tokio::time::timeout(STORE_OP_TIMEOUT, self.repo.clear_platform(platform)).await {
            Ok(Ok(0)) => {}
            Ok(Ok(_)) => self.advance(),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, ?platform, "follow knowledge clear failed");
            }
            Err(_) => tracing::warn!(?platform, "follow knowledge clear timed out"),
        }
    }

    fn advance(&self) {
        self.revision
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }
}
