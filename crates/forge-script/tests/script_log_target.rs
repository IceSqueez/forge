#![allow(clippy::unwrap_used)]
use forge_events::test_support::NullPublisher;
use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_script::{
    Engine, EngineConfig, ForgeApi, GlobalDeletion, GlobalIncrement, ScriptError, ScriptGlobals,
};
use forge_test_support::log_capture::{CapturedLine, capture};
use forge_types::{EventId, SCRIPT_LOG_TARGET, Variant};
use tracing::Level;

struct UnusedGlobals;

fn unused() -> ScriptError {
    ScriptError::DescribeOnly
}

#[async_trait::async_trait]
impl ScriptGlobals for UnusedGlobals {
    async fn get(&self, _key: &str) -> Result<Option<Variant>, ScriptError> {
        Err(unused())
    }

    async fn set(
        &self,
        _key: &str,
        _value: Variant,
        _persisted: bool,
        _caused_by: EventId,
    ) -> Result<Option<Variant>, ScriptError> {
        Err(unused())
    }

    async fn increment(
        &self,
        _key: &str,
        _delta: i64,
        _caused_by: EventId,
    ) -> Result<GlobalIncrement, ScriptError> {
        Err(unused())
    }

    async fn delete(&self, _key: &str, _caused_by: EventId) -> Result<GlobalDeletion, ScriptError> {
        Err(unused())
    }
}

fn engine() -> Engine {
    let api = ForgeApi::new(
        Arc::new(NullPublisher),
        Arc::new(UnusedGlobals),
        EventId::new(),
        Instant::now() + Duration::from_secs(10),
    );
    Engine::with_api(
        EngineConfig {
            wall_time_ms: 30_000,
            ..EngineConfig::default()
        },
        api,
    )
}

#[tokio::test]
async fn every_script_log_native_emits_on_the_script_target() {
    let engine = engine();

    let (_, lines) = capture(Level::TRACE, || {
        for native in ["log", "warn", "error"] {
            let _evaluated = engine
                .eval_script(&format!(r#"forge::{native}("marker-{native}")"#))
                .unwrap();
        }
    });

    for native in ["log", "warn", "error"] {
        let marker = format!("marker-{native}");
        let emitted: Vec<&CapturedLine> = lines
            .iter()
            .filter(|line| line.message() == marker)
            .collect();

        assert_eq!(
            emitted.len(),
            1,
            "forge::{native} must emit exactly one line, got {}",
            emitted.len()
        );
        assert_eq!(
            emitted[0].target, SCRIPT_LOG_TARGET,
            "forge::{native} escaped the script section of the diagnostic bundle"
        );
    }
}
