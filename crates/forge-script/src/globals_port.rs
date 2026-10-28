use forge_types::{EventId, Variant};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ScriptGlobalsError(pub String);

#[derive(Debug, Clone, PartialEq)]
pub struct GlobalIncrement {
    pub previous: Option<Variant>,
    pub current: Variant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GlobalDeletion {
    Missing,
    Deleted { previous: Option<Variant> },
}

#[async_trait::async_trait]
pub trait ScriptGlobals: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Variant>, ScriptGlobalsError>;

    async fn set(
        &self,
        key: &str,
        value: Variant,
        persisted: bool,
        caused_by: EventId,
    ) -> Result<Option<Variant>, ScriptGlobalsError>;

    async fn increment(
        &self,
        key: &str,
        delta: i64,
        caused_by: EventId,
    ) -> Result<GlobalIncrement, ScriptGlobalsError>;

    async fn delete(
        &self,
        key: &str,
        caused_by: EventId,
    ) -> Result<GlobalDeletion, ScriptGlobalsError>;
}
