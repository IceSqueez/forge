use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_script::{
    Engine, ForgeApi, ScriptError, ScriptHttpClient, build_scope_for_contract,
    load_script_engine_config, load_script_http_config,
};
use forge_storage::{GlobalsRepo, SettingsRepo};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use serde_json::json;
use time::OffsetDateTime;

use crate::script_registry::ScriptRegistry;

pub struct ScriptRunInlineRunner {
    registry: Arc<ScriptRegistry>,
    globals: Arc<dyn GlobalsRepo>,
    publisher: Arc<dyn EventPublisher>,
    settings: Arc<dyn SettingsRepo>,
}

impl ScriptRunInlineRunner {
    pub fn new(
        registry: Arc<ScriptRegistry>,
        globals: Arc<dyn GlobalsRepo>,
        publisher: Arc<dyn EventPublisher>,
        settings: Arc<dyn SettingsRepo>,
    ) -> Self {
        Self {
            registry,
            globals,
            publisher,
            settings,
        }
    }
}

#[async_trait]
impl SubActionRunner for ScriptRunInlineRunner {
    fn id(&self) -> &str {
        "script.run.inline"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Scripts
    }

    fn label(&self) -> &str {
        "Run Inline Script"
    }

    fn summary(&self) -> &str {
        "Execute a rhai script snippet embedded directly in the config"
    }

    fn search_text(&self) -> &str {
        "run script inline execute rhai code snippet"
    }

    fn icon_name(&self) -> &str {
        "code"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("body".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::TextArea {
            key: "body",
            label: "Script Body",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("body").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "script.run.inline: body is required".to_owned(),
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

        let body = config
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        let publisher_arc = Arc::clone(&self.publisher);
        let globals_arc = Arc::clone(&self.globals);
        let speak_requester = self.registry.speak_requester();
        let parent_event_id = ctx.parent_event_id;
        let arg_stack_clone = ctx.arg_stack.clone();
        let http_cfg = Arc::new(load_script_http_config(self.settings.as_ref()).await);
        let engine_cfg = load_script_engine_config(self.settings.as_ref()).await;

        let exec_event = Event::caused_by(
            EventSource::Rhai,
            "script.exec",
            json!({
                "script_name": "<inline>",
            }),
            parent_event_id,
        );
        let script_event_id = exec_event.id;
        self.publisher.publish(exec_event);

        let exec_result = tokio::task::spawn_blocking(move || {
            let contract = forge_types::ScriptContract::default();
            let scope = build_scope_for_contract(&contract, &arg_stack_clone).map_err(|e| {
                ScriptError::Runtime {
                    script: body.clone(),
                    reason: e.to_string(),
                }
            })?;
            let cfg = engine_cfg;
            let deadline = Instant::now() + Duration::from_millis(cfg.wall_time_ms);
            // reqwest::blocking::Client must be built inside spawn_blocking — constructing it on
            // the outer async task causes a runtime conflict on drop.
            let http_client =
                ScriptHttpClient::new(http_cfg).map_err(|e| ScriptError::Runtime {
                    script: body.clone(),
                    reason: e.to_string(),
                })?;
            let mut api = ForgeApi::new(publisher_arc, globals_arc, parent_event_id, deadline)
                .with_http(Arc::new(http_client));
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
                        "script_name": "<inline>",
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
                        "script_name": "<inline>",
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
                kind: "script.run.inline".to_owned(),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::{EventBus, NullEventLogRepo, ScriptRegistry};
    use forge_events::EventPublisher;
    use forge_storage::{GlobalsRepo, SettingsRepo};
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::EventId;

    async fn make_backend() -> Arc<SqliteBackend> {
        Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn http_module_wired_denied_domain_returns_failed() {
        let backend = make_backend().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let publisher: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
        let globals: Arc<dyn GlobalsRepo> = Arc::clone(&backend) as Arc<dyn GlobalsRepo>;
        let settings: Arc<dyn SettingsRepo> = Arc::clone(&backend) as Arc<dyn SettingsRepo>;

        let runner = ScriptRunInlineRunner::new(
            Arc::new(ScriptRegistry::new()),
            globals,
            publisher,
            settings,
        );

        let body = r#"forge::http::get("https://nope.invalid")"#;
        let mut config = runner.default_config();
        config.insert("body".to_owned(), Variant::String(body.to_owned()));

        let arg_stack = ArgStack::default();
        let dummy_pub: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
        let ctx =
            forge_registry::RunContext::leaf(&arg_stack, 0, EventId::new(), dummy_pub.as_ref());

        let (telemetry, _) = runner.execute(&config, &ctx).await;

        let msg = match telemetry.outcome {
            SubActionOutcome::Failed(m) => m,
            other => panic!("expected Failed, got {other:?}"),
        };
        assert!(
            msg.contains("domain not allowed"),
            "expected domain-not-allowed error from wired http module, got: {msg}"
        );
    }
}
