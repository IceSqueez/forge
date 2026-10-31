#![allow(clippy::unwrap_used)]
use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_twitch::{
    HelixError, HelixRequest, HelixTransport, TwitchLifecycle, register_twitch_sub_actions,
};
use forge_registry::SubActionRegistry;
use forge_storage::test_support::MemoryCredentials;
use serde_json::Value;

const STORED_DEFAULTS: &str = include_str!("fixtures/stored_step_defaults.json");

struct UnreachableTransport;

#[async_trait]
impl HelixTransport for UnreachableTransport {
    async fn execute(&self, _: HelixRequest) -> Result<Value, HelixError> {
        Ok(Value::Null)
    }
}

fn twitch_registry() -> SubActionRegistry {
    let mut registry = SubActionRegistry::new();
    register_twitch_sub_actions(
        &mut registry,
        Arc::new(UnreachableTransport),
        Arc::new(MemoryCredentials::default()),
        TwitchLifecycle::new(),
        None,
    )
    .unwrap();
    registry
}

fn stored_defaults(registry: &SubActionRegistry) -> BTreeMap<String, Value> {
    registry
        .all()
        .map(|runner| {
            (
                runner.id().to_owned(),
                serde_json::to_value(runner.default_config()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn every_twitch_step_stores_its_pinned_default_config() {
    let expected: BTreeMap<String, Value> = serde_json::from_str(STORED_DEFAULTS).unwrap();

    let actual = stored_defaults(&twitch_registry());

    assert_eq!(
        actual.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>()
    );
    for (id, defaults) in &expected {
        assert_eq!(&actual[id], defaults, "{id}");
    }
}
