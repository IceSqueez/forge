mod policy;
mod writer;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_platform_core::{FollowLookup, FollowStatus};
use forge_registry::TriggerRegistry;
use forge_storage::{EventLogRepo, FollowKnowledgeKey, FollowKnowledgeRepo, KnownFollow};
use forge_types::{PlatformId, RecoverMutex};
use time::OffsetDateTime;
use tokio::sync::{mpsc, oneshot, watch};

use self::policy::{FollowTranslation, is_fresh, known, observed, status};
use self::writer::{FollowWriter, STORE_OP_TIMEOUT, WriteCommand};
use crate::delivery::FOLLOW_KNOWLEDGE;
use crate::event_flow::bus::EventBus;

const LOOKUP_TIMEOUT: Duration = Duration::from_secs(30);
const WRITE_QUEUE_CAPACITY: usize = 64;

type InFlightLookups = Mutex<HashMap<FollowKnowledgeKey, watch::Receiver<Option<FollowStatus>>>>;

pub struct FollowKnowledgeParts {
    pub bus: Arc<EventBus>,
    pub repo: Arc<dyn FollowKnowledgeRepo>,
    pub event_log: Arc<dyn EventLogRepo>,
    pub triggers: Arc<TriggerRegistry>,
}

pub fn spawn_follow_knowledge(parts: FollowKnowledgeParts) -> FollowKnowledgeHandle {
    let FollowKnowledgeParts {
        bus,
        repo,
        event_log,
        triggers,
    } = parts;
    let revision = Arc::new(watch::Sender::new(0));
    let (writes, commands) = mpsc::channel(WRITE_QUEUE_CAPACITY);
    let writer = FollowWriter {
        repo: Arc::clone(&repo),
        translation: FollowTranslation::new(triggers),
        revision: Arc::clone(&revision),
    };
    let intake = bus.subscribe_critical(FOLLOW_KNOWLEDGE);
    let ticket = bus.flush_ticket();
    tokio::spawn(writer.run(event_log, intake, commands, ticket));
    FollowKnowledgeHandle {
        repo,
        writes,
        revision,
        lookups: Arc::default(),
    }
}

#[derive(Clone)]
pub struct FollowKnowledgeHandle {
    repo: Arc<dyn FollowKnowledgeRepo>,
    writes: mpsc::Sender<WriteCommand>,
    revision: Arc<watch::Sender<u64>>,
    lookups: Arc<InFlightLookups>,
}

impl FollowKnowledgeHandle {
    pub fn changes(&self) -> FollowKnowledgeChanges {
        FollowKnowledgeChanges(self.revision.subscribe())
    }

    pub async fn peek(&self, platform: PlatformId, viewer_id: &str) -> Option<FollowStatus> {
        self.fresh(&key_of(platform, viewer_id)).await.map(status)
    }

    pub async fn resolve(
        &self,
        platform: PlatformId,
        viewer_id: &str,
        lookup: &dyn FollowLookup,
    ) -> FollowStatus {
        let key = key_of(platform, viewer_id);
        if let Some(follow) = self.fresh(&key).await {
            return status(follow);
        }
        loop {
            match self.join_or_lead(&key) {
                Flight::Follower(mut answer) => {
                    let shared = answer
                        .wait_for(Option::is_some)
                        .await
                        .ok()
                        .and_then(|answer| *answer);
                    if let Some(shared) = shared {
                        return shared;
                    }
                }
                Flight::Leader(lead) => {
                    let answer = match self.fresh(&key).await {
                        Some(follow) => status(follow),
                        None => self.look_up(key, lookup).await,
                    };
                    lead.finish(answer);
                    return answer;
                }
            }
        }
    }

    pub async fn clear_platform(&self, platform: PlatformId) {
        let (reply, done) = oneshot::channel();
        if self
            .writes
            .send(WriteCommand::Clear { platform, reply })
            .await
            .is_err()
            || done.await.is_err()
        {
            tracing::warn!(?platform, "follow knowledge writer is gone; clear skipped");
        }
    }

    async fn fresh(&self, key: &FollowKnowledgeKey) -> Option<KnownFollow> {
        match tokio::time::timeout(STORE_OP_TIMEOUT, self.repo.get(key)).await {
            Ok(Ok(entry)) => entry
                .filter(|entry| is_fresh(entry, OffsetDateTime::now_utc()))
                .map(|entry| entry.follow),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, platform = ?key.platform, "follow knowledge read failed");
                None
            }
            Err(_) => {
                tracing::warn!(platform = ?key.platform, "follow knowledge read timed out");
                None
            }
        }
    }

    async fn look_up(&self, key: FollowKnowledgeKey, lookup: &dyn FollowLookup) -> FollowStatus {
        let issued_at = OffsetDateTime::now_utc();
        let answer = tokio::time::timeout(LOOKUP_TIMEOUT, lookup.follow_status(&key.viewer_id))
            .await
            .unwrap_or(FollowStatus::Unavailable);
        let Some(follow) = known(answer) else {
            return answer;
        };
        let (reply, winner) = oneshot::channel();
        let entry = observed(key, follow, issued_at);
        if self
            .writes
            .send(WriteCommand::Record { entry, reply })
            .await
            .is_err()
        {
            return answer;
        }
        winner.await.map_or(answer, status)
    }

    fn join_or_lead(&self, key: &FollowKnowledgeKey) -> Flight {
        let mut lookups = self.lookups.lock_recover();
        if let Some(answer) = lookups.get(key) {
            return Flight::Follower(answer.clone());
        }
        let (result, answer) = watch::channel(None);
        lookups.insert(key.clone(), answer);
        Flight::Leader(Lead {
            lookups: Arc::clone(&self.lookups),
            key: key.clone(),
            result,
        })
    }
}

pub struct FollowKnowledgeChanges(watch::Receiver<u64>);

impl FollowKnowledgeChanges {
    pub async fn changed(&mut self) -> bool {
        self.0.changed().await.is_ok()
    }
}

enum Flight {
    Leader(Lead),
    Follower(watch::Receiver<Option<FollowStatus>>),
}

struct Lead {
    lookups: Arc<InFlightLookups>,
    key: FollowKnowledgeKey,
    result: watch::Sender<Option<FollowStatus>>,
}

impl Lead {
    fn finish(self, answer: FollowStatus) {
        self.result.send_replace(Some(answer));
    }
}

impl Drop for Lead {
    fn drop(&mut self) {
        self.lookups.lock_recover().remove(&self.key);
    }
}

fn key_of(platform: PlatformId, viewer_id: &str) -> FollowKnowledgeKey {
    FollowKnowledgeKey {
        platform,
        viewer_id: viewer_id.to_owned(),
    }
}
