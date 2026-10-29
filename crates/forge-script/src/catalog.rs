use std::sync::{Arc, LazyLock};
use std::time::Instant;

use forge_events::{Event, EventPublisher};
use forge_types::{EventId, Variant};

use crate::api::ForgeApi;
use crate::globals_port::{GlobalDeletion, GlobalIncrement, ScriptGlobals, ScriptGlobalsError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamDescriptor {
    pub name: &'static str,
    pub ty: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodDescriptor {
    pub namespace: Option<&'static str>,
    pub name: &'static str,
    pub params: Vec<ParamDescriptor>,
    pub return_type: &'static str,
    pub doc: Option<&'static str>,
}

const DESCRIBE_ONLY_REASON: &str = "the script catalog never runs script functions";

struct DescribeOnly;

impl EventPublisher for DescribeOnly {
    fn publish(&self, _event: Event) {}
}

#[async_trait::async_trait]
impl ScriptGlobals for DescribeOnly {
    async fn get(&self, _key: &str) -> Result<Option<Variant>, ScriptGlobalsError> {
        Err(describe_only())
    }

    async fn set(
        &self,
        _key: &str,
        _value: Variant,
        _persisted: bool,
        _caused_by: EventId,
    ) -> Result<Option<Variant>, ScriptGlobalsError> {
        Err(describe_only())
    }

    async fn increment(
        &self,
        _key: &str,
        _delta: i64,
        _caused_by: EventId,
    ) -> Result<GlobalIncrement, ScriptGlobalsError> {
        Err(describe_only())
    }

    async fn delete(
        &self,
        _key: &str,
        _caused_by: EventId,
    ) -> Result<GlobalDeletion, ScriptGlobalsError> {
        Err(describe_only())
    }
}

fn describe_only() -> ScriptGlobalsError {
    ScriptGlobalsError(DESCRIBE_ONLY_REASON.to_owned())
}

static CATALOG: LazyLock<Vec<MethodDescriptor>> = LazyLock::new(|| {
    ForgeApi::new(
        Arc::new(DescribeOnly),
        Arc::new(DescribeOnly),
        EventId::new(),
        Instant::now(),
    )
    .describe()
});

pub fn catalog() -> &'static [MethodDescriptor] {
    &CATALOG
}

pub fn is_engine_bound_name(name: &str) -> bool {
    catalog()
        .iter()
        .any(|method| method.namespace.unwrap_or(method.name) == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_globals_get() {
        assert!(
            catalog()
                .iter()
                .any(|d| d.namespace == Some("globals") && d.name == "get"),
            "catalog must contain globals::get"
        );
    }

    #[test]
    fn catalog_globals_get_signature_matches_register_fn() {
        let entry = catalog()
            .iter()
            .find(|d| d.namespace == Some("globals") && d.name == "get")
            .expect("globals::get must be in catalog");
        assert_eq!(entry.params.len(), 1);
        assert_eq!(entry.params[0].name, "key");
        assert_eq!(entry.params[0].ty, "string");
        assert_eq!(entry.return_type, "Variant");
    }
}
