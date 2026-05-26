use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_events::{Event, EventSource};
use forge_script::{Engine, EngineConfig, ForgeApi, ScriptError, build_scope_for_contract};
use forge_storage::GlobalsRepo;
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
    globals: Arc<dyn GlobalsRepo>,
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

    let script_id = compiled.record.id;
    let body = compiled.record.body.clone();
    let contract = compiled.record.contract.clone();
    let arg_stack_clone = arg_stack.clone();
    let bus_arc: Arc<EventBus> = Arc::clone(bus);
    let publisher: Arc<dyn forge_events::EventPublisher> = bus_arc;
    let globals_for_api = Arc::clone(&globals);
    let speak_requester = registry.speak_requester();

    let exec_event = Event::caused_by(
        EventSource::Rhai,
        "script.exec",
        json!({
            "script_id": script_id.to_string(),
            "script_name": name.as_str(),
        }),
        parent_event_id,
    );
    let script_event_id = exec_event.id;
    bus.publish(exec_event);

    let exec_result = tokio::task::spawn_blocking(move || {
        let scope = build_scope_for_contract(&contract, &arg_stack_clone).map_err(|e| {
            ScriptError::Runtime {
                script: body.clone(),
                reason: e.to_string(),
            }
        })?;
        let cfg = EngineConfig::default();
        let deadline = Instant::now() + Duration::from_millis(cfg.wall_time_ms);
        let mut api = ForgeApi::new(publisher, globals_for_api, parent_event_id, deadline);
        if let Some(req) = speak_requester {
            api = api.with_speak_requester(req);
        }
        let engine = Engine::with_api(cfg, api);
        let mut scope = scope;
        engine.eval_script_with_scope(&body, &mut scope)
    })
    .await;

    let duration_ms = wall_start.elapsed().as_millis() as u64;

    let outcome = match exec_result {
        Ok(Ok(_)) => SubActionOutcome::Success,
        Ok(Err(script_err)) => {
            bus.publish(Event::caused_by(
                EventSource::Rhai,
                "script.error",
                json!({
                    "script_id": script_id.to_string(),
                    "script_name": name.as_str(),
                    "error_type": error_kind(&script_err),
                    "message": script_err.to_string(),
                }),
                script_event_id,
            ));
            SubActionOutcome::Failed(script_err.to_string())
        }
        Err(join_err) => {
            bus.publish(Event::caused_by(
                EventSource::Rhai,
                "script.error",
                json!({
                    "script_id": script_id.to_string(),
                    "script_name": name.as_str(),
                    "error_type": "panic",
                    "message": format!("script task panicked: {join_err}"),
                }),
                script_event_id,
            ));
            SubActionOutcome::Failed(format!("script task panicked: {join_err}"))
        }
    };

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

fn error_kind(err: &ScriptError) -> &'static str {
    match err {
        ScriptError::Compile { .. } => "syntax",
        ScriptError::Timeout { .. } => "timeout",
        ScriptError::OperationLimit { .. } => "ops_exceeded",
        _ => "runtime",
    }
}
