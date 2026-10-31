use crate::error::ScriptError;
use forge_types::{EventId, Variant};

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
    async fn get(&self, key: &str) -> Result<Option<Variant>, ScriptError>;

    async fn set(
        &self,
        key: &str,
        value: Variant,
        persisted: bool,
        caused_by: EventId,
    ) -> Result<Option<Variant>, ScriptError>;

    async fn increment(
        &self,
        key: &str,
        delta: i64,
        caused_by: EventId,
    ) -> Result<GlobalIncrement, ScriptError>;

    async fn delete(&self, key: &str, caused_by: EventId) -> Result<GlobalDeletion, ScriptError>;
}

#[cfg(test)]
mod tests {
    use forge_storage::StorageError;

    use super::*;

    #[test]
    fn a_storage_failure_reaches_the_script_verbatim() {
        let error = ScriptError::from(StorageError::TypeMismatch {
            name: "hits".to_owned(),
            actual: "string".to_owned(),
        });

        assert!(matches!(error, ScriptError::Storage(_)));
        assert_eq!(
            error.to_string(),
            "global 'hits' has type string, expected numeric"
        );
    }
}
