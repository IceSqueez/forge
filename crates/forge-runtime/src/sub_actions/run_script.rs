use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_events::{Event, EventSource};
use forge_script::{Engine, EngineConfig, ForgeApi, build_scope_for_contract};
use forge_storage::DataProvider;
use forge_types::{ArgStack, EventId, SubActionOutcome, SubActionTelemetry};
use serde_json::json;
use time::OffsetDateTime;

use crate::EventBus;
use crate::script_registry::ScriptRegistry;

pub(super) async fn run(
    script_name: &str,
    arg_stack: &ArgStack,
    index: usize,
    parent_event_id: EventId,
    bus: &Arc<EventBus>,
    dp: Arc<dyn DataProvider>,
    registry: &ScriptRegistry,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let name = arg_stack.interpolate(script_name);
    let started_at = OffsetDateTime::now_utc();
    let wall_start = Instant::now();

    let Some(compiled) = registry.get_by_name(&name).await else {
        let duration_ms = wall_start.elapsed().as_millis() as u64;
        return (
            SubActionTelemetry {
                kind: "RunScript".to_string(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Failed(format!("script '{name}' not found in registry")),
                index,
            },
            None,
        );
    };

    let body = compiled.record.body.clone();
    let contract = compiled.record.contract.clone();
    let arg_stack_clone = arg_stack.clone();
    let caused_by = parent_event_id;
    let bus_arc: Arc<EventBus> = Arc::clone(bus);
    let publisher: Arc<dyn forge_events::EventPublisher> = bus_arc;
    let dp_for_api = Arc::clone(&dp);

    let exec_result = tokio::task::spawn_blocking(move || {
        let scope = build_scope_for_contract(&contract, &arg_stack_clone).map_err(|e| {
            forge_script::ScriptError::Runtime {
                script: body.clone(),
                reason: e.to_string(),
            }
        })?;
        let cfg = EngineConfig::default();
        let deadline = Instant::now() + Duration::from_millis(cfg.wall_time_ms);
        let api = ForgeApi::new(publisher, dp_for_api, caused_by, deadline);
        let engine = Engine::with_api(cfg, api);
        let mut scope = scope;
        engine.eval_script_with_scope(&body, &mut scope)
    })
    .await;

    let duration_ms = wall_start.elapsed().as_millis() as u64;

    let (event_kind, outcome) = match exec_result {
        Ok(Ok(_)) => ("script.exec", SubActionOutcome::Success),
        Ok(Err(script_err)) => (
            "script.error",
            SubActionOutcome::Failed(script_err.to_string()),
        ),
        Err(join_err) => (
            "script.error",
            SubActionOutcome::Failed(format!("script task panicked: {join_err}")),
        ),
    };

    let payload = match &outcome {
        SubActionOutcome::Success => json!({
            "script_name": name,
            "duration_ms": duration_ms,
        }),
        SubActionOutcome::Failed(msg) => json!({
            "script_name": name,
            "error": msg,
            "duration_ms": duration_ms,
        }),
        SubActionOutcome::Skipped(_) => unreachable!(),
    };

    bus.publish(Event::caused_by(
        EventSource::Rhai,
        event_kind,
        payload,
        parent_event_id,
    ));

    (
        SubActionTelemetry {
            kind: "RunScript".to_string(),
            started_at,
            duration_ms,
            outcome,
            index,
        },
        None,
    )
}
