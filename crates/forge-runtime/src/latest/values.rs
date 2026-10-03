use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use forge_events::{Event, EventPublisher, LatestChanged};
use forge_storage::{LatestRecord, LatestValueRepo, StorageError};
use forge_types::{EventId, LatestScope, LatestValue, LatestValueReader, Shared, Variant};
use time::OffsetDateTime;

use crate::latest::slots::{
    FeedContext, LATEST_SLOTS, LatestSlotDeclaration, SlotReading, SlotRetention, slot_declaration,
};

const STORAGE_OP_TIMEOUT: Duration = Duration::from_secs(5);
const PERSIST_ATTEMPTS: u32 = 3;
const PERSIST_RETRY_DELAY: Duration = Duration::from_millis(500);
const TEST_MARKER: &str = "test";

type SlotValues = BTreeMap<String, LatestValue>;

#[derive(Clone)]
pub struct LatestValues {
    values: Arc<RwLock<HashMap<&'static str, SlotValues>>>,
    repo: Arc<dyn LatestValueRepo>,
    publisher: Arc<dyn EventPublisher>,
    anonymous_name: Shared<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LatestResetError {
    #[error("unknown latest slot '{0}'")]
    UnknownSlot(String),
    #[error("latest slot could not be reset: {0}")]
    Storage(#[from] StorageError),
}

impl LatestValues {
    pub async fn load(
        repo: Arc<dyn LatestValueRepo>,
        publisher: Arc<dyn EventPublisher>,
        anonymous_name: Shared<String>,
    ) -> Self {
        let mut values = HashMap::new();
        for declaration in LATEST_SLOTS {
            if declaration.retention == SlotRetention::SurvivesRestart {
                values.insert(
                    declaration.slot,
                    stored_values(repo.as_ref(), declaration).await,
                );
            }
        }
        Self {
            values: Arc::new(RwLock::new(values)),
            repo,
            publisher,
            anonymous_name,
        }
    }

    pub fn latest(&self, slot: &str, scope: LatestScope<'_>) -> Option<LatestValue> {
        let values = self.values.read().unwrap_or_else(PoisonError::into_inner);
        let slot_values = values.get(slot)?;
        match scope {
            LatestScope::Platform(platform) => slot_values.get(platform).cloned(),
            LatestScope::MostRecentAcrossPlatforms => most_recent(slot_values).cloned(),
        }
    }

    pub async fn reset(&self, slot: &str) -> Result<(), LatestResetError> {
        let declaration =
            slot_declaration(slot).ok_or_else(|| LatestResetError::UnknownSlot(slot.to_owned()))?;
        if declaration.retention == SlotRetention::SurvivesRestart {
            with_timeout(self.repo.reset_slot(declaration.slot)).await?;
        }
        let removed = {
            let mut values = self.values.write().unwrap_or_else(PoisonError::into_inner);
            values
                .remove(declaration.slot)
                .is_some_and(|slot_values| !slot_values.is_empty())
        };
        if removed {
            self.announce(
                LatestChanged {
                    slot: declaration.slot.to_owned(),
                    platform: None,
                    merged_changed: true,
                    cleared: true,
                },
                None,
            );
        }
        Ok(())
    }

    pub(crate) async fn project(&self, event: &Event) {
        if !admits(event) {
            return;
        }
        let anonymous_name = self.anonymous_name.load();
        let context = FeedContext {
            anonymous_name: anonymous_name.as_str(),
        };
        for declaration in LATEST_SLOTS {
            for feed in declaration
                .feeds
                .iter()
                .filter(|feed| feed.kind == event.kind)
            {
                if let Some(reading) = (feed.read)(event, &context) {
                    self.settle(declaration, reading, event.id).await;
                }
            }
        }
    }

    async fn settle(
        &self,
        declaration: &'static LatestSlotDeclaration,
        reading: SlotReading,
        cause: EventId,
    ) {
        match reading {
            SlotReading::Value(value) => self.accept(declaration, value, cause).await,
            SlotReading::Ended { platform } => {
                if declaration.retention == SlotRetention::LiveOnly {
                    self.end(declaration.slot, platform, cause);
                }
            }
        }
    }

    async fn accept(
        &self,
        declaration: &'static LatestSlotDeclaration,
        value: LatestValue,
        cause: EventId,
    ) {
        if !self.is_newer(declaration.slot, &value) {
            return;
        }
        if declaration.retention == SlotRetention::SurvivesRestart
            && !self.persist(declaration.slot, &value).await
        {
            return;
        }
        let platform = value.platform.clone();
        let merged_changed = {
            let mut values = self.values.write().unwrap_or_else(PoisonError::into_inner);
            let slot_values = values.entry(declaration.slot).or_default();
            let before = merged_identity(slot_values);
            slot_values.insert(platform.clone(), value);
            before != merged_identity(slot_values)
        };
        self.announce(
            LatestChanged {
                slot: declaration.slot.to_owned(),
                platform: Some(platform),
                merged_changed,
                cleared: false,
            },
            Some(cause),
        );
    }

    fn end(&self, slot: &'static str, platform: String, cause: EventId) {
        let merged_changed = {
            let mut values = self.values.write().unwrap_or_else(PoisonError::into_inner);
            let Some(slot_values) = values.get_mut(slot) else {
                return;
            };
            let before = merged_identity(slot_values);
            if slot_values.remove(&platform).is_none() {
                return;
            }
            before != merged_identity(slot_values)
        };
        self.announce(
            LatestChanged {
                slot: slot.to_owned(),
                platform: Some(platform),
                merged_changed,
                cleared: true,
            },
            Some(cause),
        );
    }

    fn is_newer(&self, slot: &str, value: &LatestValue) -> bool {
        let values = self.values.read().unwrap_or_else(PoisonError::into_inner);
        values
            .get(slot)
            .and_then(|slot_values| slot_values.get(&value.platform))
            .is_none_or(|stored| value.occurred_at > stored.occurred_at)
    }

    async fn persist(&self, slot: &str, value: &LatestValue) -> bool {
        let record = LatestRecord {
            slot: slot.to_owned(),
            platform: value.platform.clone(),
            payload: value.to_variant().to_json(),
            occurred_at: value.occurred_at,
            updated_at: OffsetDateTime::now_utc(),
        };
        let mut attempt = 1;
        loop {
            match with_timeout(self.repo.upsert_if_newer(&record)).await {
                Ok(stored) => return stored,
                Err(error) if attempt >= PERSIST_ATTEMPTS => {
                    tracing::error!(
                        slot,
                        platform = %value.platform,
                        error = %error,
                        "latest value could not be saved; it shows until restart"
                    );
                    return true;
                }
                Err(error) => {
                    tracing::warn!(slot, attempt, error = %error, "saving a latest value failed; retrying");
                    attempt += 1;
                    tokio::time::sleep(PERSIST_RETRY_DELAY).await;
                }
            }
        }
    }

    fn announce(&self, change: LatestChanged, cause: Option<EventId>) {
        match change.into_event(cause) {
            Ok(event) => self.publisher.publish(event),
            Err(error) => tracing::warn!(error = %error, "latest change event not encodable"),
        }
    }
}

impl LatestValueReader for LatestValues {
    fn latest(&self, slot: &str, scope: LatestScope<'_>) -> Option<LatestValue> {
        LatestValues::latest(self, slot, scope)
    }
}

fn admits(event: &Event) -> bool {
    let test_marked = event
        .payload
        .get(TEST_MARKER)
        .and_then(|marker| marker.as_bool())
        .unwrap_or(false);
    !event.replay && !test_marked
}

fn most_recent(slot_values: &SlotValues) -> Option<&LatestValue> {
    slot_values.values().max_by_key(|value| value.occurred_at)
}

fn merged_identity(slot_values: &SlotValues) -> Option<(String, OffsetDateTime)> {
    most_recent(slot_values).map(|value| (value.platform.clone(), value.occurred_at))
}

async fn with_timeout<T>(
    operation: impl Future<Output = Result<T, StorageError>>,
) -> Result<T, StorageError> {
    tokio::time::timeout(STORAGE_OP_TIMEOUT, operation)
        .await
        .unwrap_or_else(|_| {
            Err(StorageError::Connection {
                reason: "latest value storage timed out".to_owned(),
            })
        })
}

async fn stored_values(
    repo: &dyn LatestValueRepo,
    declaration: &LatestSlotDeclaration,
) -> SlotValues {
    let records = match with_timeout(repo.list_slot(declaration.slot)).await {
        Ok(records) => records,
        Err(error) => {
            tracing::warn!(slot = declaration.slot, error = %error, "stored latest values unreadable; slot starts empty");
            return SlotValues::new();
        }
    };
    records
        .into_iter()
        .filter_map(|record| match Variant::from_json(record.payload) {
            Ok(Variant::Object(fields)) => Some((
                record.platform.clone(),
                LatestValue::new(record.platform, record.occurred_at, fields),
            )),
            _ => {
                tracing::warn!(
                    slot = declaration.slot,
                    platform = %record.platform,
                    "stored latest value is malformed; skipped"
                );
                None
            }
        })
        .collect()
}
