use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use forge_events::Event;
use forge_storage::{
    EventLogRepo, FollowKnowledgeEntry, FollowKnowledgeKey, FollowKnowledgeRepo, KnownFollow,
};
use forge_types::PlatformId;
use time::OffsetDateTime;
use tokio::sync::{mpsc, oneshot, watch};

use super::policy::{FOLLOWING_LIFETIME, FollowTranslation, is_fresh};
use crate::delivery::CriticalSubscription;
use crate::event_flow::persist_batch::FlushTicket;

pub(super) const STORE_OP_TIMEOUT: Duration = Duration::from_secs(5);
const STARTUP_OP_TIMEOUT: Duration = Duration::from_secs(30);

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
        mut ticket: FlushTicket,
    ) {
        let started = tokio::select! {
            biased;
            () = ticket.stopping() => false,
            () = self.start(event_log.as_ref()) => true,
        };
        drop(event_log);
        if started {
            self.serve(&mut intake, &mut commands, &mut ticket).await;
            self.drain(&mut intake, &mut ticket).await;
        }
        ticket.finish();
    }

    async fn start(&self, event_log: &dyn EventLogRepo) {
        self.prune().await;
        self.seed(event_log).await;
    }

    async fn serve(
        &self,
        intake: &mut CriticalSubscription,
        commands: &mut mpsc::Receiver<WriteCommand>,
        ticket: &mut FlushTicket,
    ) {
        let mut intake_open = true;
        let mut commands_open = true;
        while intake_open || commands_open {
            tokio::select! {
                biased;
                () = ticket.stopping() => return,
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

    async fn drain(&self, intake: &mut CriticalSubscription, ticket: &mut FlushTicket) {
        while let Some(event) = intake.try_recv() {
            tokio::select! {
                biased;
                () = ticket.abandoning() => return,
                () = self.take(&event) => {}
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
        let since = OffsetDateTime::now_utc().saturating_sub(FOLLOWING_LIFETIME);
        let read = event_log.events_of_kinds_since(self.translation.logged_follow_kinds(), since);
        let events = match tokio::time::timeout(STARTUP_OP_TIMEOUT, read).await {
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
        let entries = self.newest_entries(&events);
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

    fn newest_entries(&self, events: &[Event]) -> Vec<FollowKnowledgeEntry> {
        let mut newest: HashMap<FollowKnowledgeKey, FollowKnowledgeEntry> = HashMap::new();
        for entry in events
            .iter()
            .filter_map(|event| self.translation.entry_of(event))
        {
            newest.insert(entry.key.clone(), entry);
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
