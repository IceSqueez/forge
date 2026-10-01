use std::collections::BTreeMap;
use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::builtin::{QuickActionField, QuickActionFieldValue, SectionIcon};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CollectionId(pub String);

impl CollectionId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CollectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CollectionItemId(pub String);

impl CollectionItemId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CollectionItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollectionField {
    pub field: QuickActionField,
    pub max_chars: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionToggle {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollectionMetadata {
    pub id: CollectionId,
    pub label: String,
    pub icon: SectionIcon,
    pub capacity: Option<usize>,
    pub fields: Vec<CollectionField>,
    pub toggles: Vec<CollectionToggle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionItemAccess {
    Manageable,
    ReadOnly { reason: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollectionItem {
    pub id: CollectionItemId,
    pub title: String,
    pub values: BTreeMap<String, QuickActionFieldValue>,
    pub toggles: BTreeMap<String, bool>,
    pub access: CollectionItemAccess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionFailure {
    NotConnected,
    Unauthorized,
    NotOwned,
    CapacityReached,
    NotEligible,
    InvalidInput {
        field: Option<String>,
        message: String,
    },
    RateLimited,
    Transport,
}

impl fmt::Display for CollectionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Self::NotConnected => "not connected",
            Self::Unauthorized => "authorization expired or revoked",
            Self::NotOwned => "item was created elsewhere and cannot be changed here",
            Self::CapacityReached => "collection is full",
            Self::NotEligible => "channel is not eligible for this feature",
            Self::InvalidInput { message, .. } => message,
            Self::RateLimited => "rate limited, try again shortly",
            Self::Transport => "connection transport error",
        };
        f.write_str(reason)
    }
}

impl std::error::Error for CollectionFailure {}

pub type CollectionOutcome<T> = Result<T, CollectionFailure>;

#[derive(Debug)]
pub struct CollectionRevisionSignal(watch::Sender<u64>);

impl CollectionRevisionSignal {
    pub fn new() -> Self {
        Self(watch::Sender::new(0))
    }

    pub fn bump(&self) {
        self.0
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    pub fn subscribe(&self) -> CollectionRevisions {
        CollectionRevisions(self.0.subscribe())
    }
}

impl Default for CollectionRevisionSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionWait {
    Changed,
    SignalDropped,
}

#[derive(Debug, Clone)]
pub struct CollectionRevisions(watch::Receiver<u64>);

impl CollectionRevisions {
    pub async fn changed(&mut self) -> RevisionWait {
        match self.0.changed().await {
            Ok(()) => RevisionWait::Changed,
            Err(_) => RevisionWait::SignalDropped,
        }
    }
}

#[async_trait]
pub trait BuiltinCollections: Send + Sync {
    fn collections(&self) -> Vec<CollectionMetadata>;

    fn revisions(&self) -> CollectionRevisions;

    async fn list(&self, collection: &CollectionId) -> CollectionOutcome<Vec<CollectionItem>>;

    async fn create(
        &self,
        collection: &CollectionId,
        values: &BTreeMap<String, QuickActionFieldValue>,
    ) -> CollectionOutcome<CollectionItem>;

    async fn update(
        &self,
        collection: &CollectionId,
        item: &CollectionItemId,
        values: &BTreeMap<String, QuickActionFieldValue>,
    ) -> CollectionOutcome<CollectionItem>;

    async fn delete(
        &self,
        collection: &CollectionId,
        item: &CollectionItemId,
    ) -> CollectionOutcome<()>;

    async fn set_toggle(
        &self,
        collection: &CollectionId,
        item: &CollectionItemId,
        toggle: &str,
        on: bool,
    ) -> CollectionOutcome<CollectionItem>;
}
