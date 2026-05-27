use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_script::{Engine, EngineConfig, ForgeApi, ScriptError, build_scope_for_contract};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use serde_json::json;
use time::OffsetDateTime;

use crate::script_registry::ScriptRegistry;

pub struct ScriptRunNamedRunner {
    registry: Arc<ScriptRegistry>,
    globals: Arc<dyn GlobalsRepo>,
    publisher: Arc<dyn EventPublisher>,
}

impl ScriptRunNamedRunner {
    pub fn new(
        registry: Arc<ScriptRegistry>,
        globals: Arc<dyn GlobalsRepo>,
        publisher: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            registry,
            globals,
            publisher,
        }
    }
}

#[async_trait]
impl SubActionRunner for ScriptRunNamedRunner {
    fn id(&self) -> &str {
        "script.run.named"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Scripts
    }

    fn label(&self) -> &str {
        "Run Script (Named)"
    }

    fn summary(&self) -> &str {
        "Execute a saved script by name"
    }

    fn search_text(&self) -> &str {
        "run script named execute rhai"
    }

    fn icon_name(&self) -> &str {
        "script"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("script_name".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "script_name",
            label: "Script",
            options_key: "script.names",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("script_name").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "script.run.named: script_name is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let wall_start = Instant::now();

        let name_template = config
            .get("script_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let name = ctx.arg_stack.interpolate(name_template);

        let Some(compiled) = self.registry.get_by_name(&name).await else {
            let duration_ms = wall_start.elapsed().as_millis() as u64;
            return (
                SubActionTelemetry {
                    kind: "script.run.named".to_owned(),
                    started_at,
                    duration_ms,
                    outcome: SubActionOutcome::Failed(format!(
                        "script '{name}' not found in registry"
                    )),
                    index: ctx.index,
                },
                None,
            );
        };

        let script_id = compiled.record.id;
        let body = compiled.record.body.clone();
        let contract = compiled.record.contract.clone();
        let arg_stack_clone = ctx.arg_stack.clone();
        let publisher_arc = Arc::clone(&self.publisher);
        let globals_arc = Arc::clone(&self.globals);
        let speak_requester = self.registry.speak_requester();
        let parent_event_id = ctx.parent_event_id;

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
        self.publisher.publish(exec_event);

        let exec_result = tokio::task::spawn_blocking(move || {
            let scope = build_scope_for_contract(&contract, &arg_stack_clone).map_err(|e| {
                ScriptError::Runtime {
                    script: body.clone(),
                    reason: e.to_string(),
                }
            })?;
            let cfg = EngineConfig::default();
            let deadline = Instant::now() + Duration::from_millis(cfg.wall_time_ms);
            let mut api = ForgeApi::new(publisher_arc, globals_arc, parent_event_id, deadline);
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
                self.publisher.publish(Event::caused_by(
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
                self.publisher.publish(Event::caused_by(
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
                kind: "script.run.named".to_owned(),
                started_at,
                duration_ms,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

fn error_kind(err: &ScriptError) -> &'static str {
    match err {
        ScriptError::Compile { .. } => "syntax",
        ScriptError::Timeout { .. } => "timeout",
        ScriptError::OperationLimit { .. } => "ops_exceeded",
        _ => "runtime",
    }
}
