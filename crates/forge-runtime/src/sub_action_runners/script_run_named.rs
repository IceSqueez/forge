use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_script::{
    Engine, ForgeApi, ScriptError, ScriptHttpClient, build_scope_for_contract,
    load_script_engine_config, load_script_http_config,
};
use forge_storage::{ExecutionStatus, GlobalsRepo, ScriptRepo, SettingsRepo};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use serde_json::json;
use time::OffsetDateTime;

use crate::script_registry::ScriptRegistry;

pub struct ScriptRunNamedRunner {
    registry: Arc<ScriptRegistry>,
    globals: Arc<dyn GlobalsRepo>,
    publisher: Arc<dyn EventPublisher>,
    settings: Arc<dyn SettingsRepo>,
    scripts: Arc<dyn ScriptRepo>,
}

impl ScriptRunNamedRunner {
    pub fn new(
        registry: Arc<ScriptRegistry>,
        globals: Arc<dyn GlobalsRepo>,
        publisher: Arc<dyn EventPublisher>,
        settings: Arc<dyn SettingsRepo>,
        scripts: Arc<dyn ScriptRepo>,
    ) -> Self {
        Self {
            registry,
            globals,
            publisher,
            settings,
            scripts,
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
        cfg.insert("target_var".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "script_name",
                label: "Script",
                options_key: "script.names",
            },
            FormField::Text {
                key: "target_var",
                label: "Output Variable",
                placeholder: "script_result",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("script_name").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let wall_start = Instant::now();

        let name_template = config.str("script_name").unwrap_or_default();
        let name = ctx.arg_stack.interpolate(name_template);

        let target_var_template = config.str("target_var").unwrap_or_default();
        let target_var =
            forge_types::strip_var_decoration(&ctx.arg_stack.interpolate(target_var_template));

        let Some(compiled) = self.registry.get_by_name(&name).await else {
            let duration_ms = wall_start.elapsed().as_millis() as u64;
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
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
        let http_cfg = Arc::new(load_script_http_config(self.settings.as_ref()).await);
        let engine_cfg = load_script_engine_config(self.settings.as_ref()).await;

        let exec_event = Event::caused_by(
            EventSource::Rhai,
            "script.exec",
            json!({
                "script_id": script_id.to_string(),
                "script_name": name.as_str(),
                "is_inline": false,
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
            let cfg = engine_cfg;
            let deadline = Instant::now() + Duration::from_millis(cfg.wall_time_ms);
            // reqwest::blocking::Client must be built inside spawn_blocking, not the outer async task.
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
            engine.eval_script_with_scope_as_variant(&body, &mut scope)
        })
        .await;

        let duration_ms = wall_start.elapsed().as_millis() as u64;

        let (outcome, updated_stack) = match exec_result {
            Ok(Ok(value)) => match value {
                Some(value) if !target_var.is_empty() => {
                    let stack = ctx.arg_stack.clone().set(target_var.clone(), value);
                    (SubActionOutcome::Success, Some(stack))
                }
                _ => (SubActionOutcome::Success, None),
            },
            Ok(Err(script_err)) => {
                let message = format!("script '{name}' failed: {}", body_free_message(&script_err));
                self.publisher.publish(Event::caused_by(
                    EventSource::Rhai,
                    "script.error",
                    json!({
                        "script_id": script_id.to_string(),
                        "script_name": name.as_str(),
                        "is_inline": false,
                        "error_type": error_kind(&script_err),
                        "message": message.as_str(),
                    }),
                    script_event_id,
                ));
                (SubActionOutcome::Failed(message), None)
            }
            Err(join_err) => {
                self.publisher.publish(Event::caused_by(
                    EventSource::Rhai,
                    "script.error",
                    json!({
                        "script_id": script_id.to_string(),
                        "script_name": name.as_str(),
                        "is_inline": false,
                        "error_type": "panic",
                        "message": format!("script task panicked: {join_err}"),
                    }),
                    script_event_id,
                ));
                (
                    SubActionOutcome::Failed(format!("script task panicked: {join_err}")),
                    None,
                )
            }
        };

        let status = if matches!(outcome, SubActionOutcome::Success) {
            ExecutionStatus::Success
        } else {
            ExecutionStatus::Error
        };
        if let Err(e) = self
            .scripts
            .record_execution(script_id, started_at, duration_ms, status)
            .await
        {
            tracing::warn!(error = %e, "script_repo.record_execution failed");
        }

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "script.run.named".to_owned(),
                started_at,
                duration_ms,
                outcome,
                index: ctx.index,
            },
            updated_stack,
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

fn body_free_message(err: &ScriptError) -> String {
    match err {
        ScriptError::Compile { reason, .. } => format!("compile error: {reason}"),
        ScriptError::Runtime { reason, .. } => format!("runtime error: {reason}"),
        ScriptError::Timeout {
            elapsed_ms,
            limit_ms,
            ..
        } => format!("timed out after {elapsed_ms}ms (limit {limit_ms}ms)"),
        ScriptError::OperationLimit { ops, .. } => {
            format!("exceeded op-count limit ({ops} operations)")
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::{EventBus, NullEventLogRepo};
    use forge_storage::ScriptRecord;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{EventId, ScriptContract, ScriptId};
    use time::OffsetDateTime;

    const BODY: &str = "let secret_token = 42; print(secret_token);";

    #[tokio::test]
    async fn named_script_exec_and_error_carry_script_identity_and_clear_inline_flag() {
        let backend = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let publisher: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;

        let script_id = ScriptId::new();
        let ts = OffsetDateTime::now_utc();
        let record = ScriptRecord {
            id: script_id,
            name: "greet".to_owned(),
            body: "throw \"boom\";".to_owned(),
            contract: ScriptContract::default(),
            body_hash: "h".to_owned(),
            enabled: true,
            created_at: ts,
            last_modified: ts,
        };
        let registry = Arc::new(ScriptRegistry::new());
        registry.reload(record, &bus).await.unwrap();

        let runner = ScriptRunNamedRunner::new(
            registry,
            Arc::clone(&backend) as Arc<dyn GlobalsRepo>,
            publisher,
            Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            Arc::clone(&backend) as Arc<dyn ScriptRepo>,
        );

        let mut config = runner.default_config();
        config.insert(
            "script_name".to_owned(),
            Variant::String("greet".to_owned()),
        );

        let mut sub = bus.subscribe();
        let arg_stack = ArgStack::default();
        let dummy_pub: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
        let ctx =
            forge_registry::RunContext::leaf(&arg_stack, 0, EventId::new(), dummy_pub.as_ref());

        let _ = runner.execute(&config, &ctx).await;

        let mut exec = None;
        let mut error = None;
        for _ in 0..40 {
            match tokio::time::timeout(Duration::from_millis(50), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == "script.exec" => exec = Some(ev),
                Ok(Ok(ev)) if ev.kind == "script.error" => error = Some(ev),
                Ok(Ok(_)) => {}
                _ => break,
            }
            if exec.is_some() && error.is_some() {
                break;
            }
        }

        let expected_id = script_id.to_string();
        for ev in [
            exec.expect("named run must emit script.exec"),
            error.expect("failing named run must emit script.error"),
        ] {
            assert_eq!(
                ev.payload["script_id"].as_str(),
                Some(expected_id.as_str()),
                "named {} must carry the resolved script_id",
                ev.kind
            );
            assert_eq!(
                ev.payload["script_name"].as_str(),
                Some("greet"),
                "named {} must carry the resolved script_name",
                ev.kind
            );
            assert_eq!(
                ev.payload["is_inline"].as_bool(),
                Some(false),
                "named {} must flag is_inline=false",
                ev.kind
            );
        }
    }

    #[test]
    fn body_free_message_reports_the_reason_without_leaking_the_script_body() {
        for (err, expected) in [
            (
                ScriptError::Compile {
                    script: BODY.to_owned(),
                    reason: "unexpected token".to_owned(),
                },
                "compile error: unexpected token",
            ),
            (
                ScriptError::Runtime {
                    script: BODY.to_owned(),
                    reason: "divide by zero".to_owned(),
                },
                "runtime error: divide by zero",
            ),
            (
                ScriptError::Timeout {
                    script: BODY.to_owned(),
                    elapsed_ms: 200,
                    limit_ms: 100,
                },
                "timed out after 200ms (limit 100ms)",
            ),
            (
                ScriptError::OperationLimit {
                    script: BODY.to_owned(),
                    ops: 5000,
                },
                "exceeded op-count limit (5000 operations)",
            ),
        ] {
            let msg = body_free_message(&err);
            assert_eq!(msg, expected);
            assert!(
                !msg.contains("secret_token"),
                "body-free message leaked the script body: {msg}",
            );
        }
    }

    #[test]
    fn default_display_leaks_the_body_that_body_free_message_strips() {
        let err = ScriptError::Compile {
            script: BODY.to_owned(),
            reason: "unexpected token".to_owned(),
        };
        assert!(err.to_string().contains("secret_token"));
        assert!(!body_free_message(&err).contains("secret_token"));
    }
}
