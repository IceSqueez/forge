use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use forge_events::{EventPublisher, EventSource, ScriptFailed, ScriptStarted, TypedEvent};
use forge_registry::references::SCRIPT_OPTIONS_KEY;
use forge_registry::{
    ConfigProblem, ConfigSpec, DynamicSelectField, ProducedVariable, RunContext, StepTimer,
    SubActionCategory, SubActionIo, SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_script::{
    Engine, EngineConfig, ForgeApi, ScriptError, ScriptGlobals, ScriptHttpClient, ScriptHttpConfig,
    build_scope_for_contract,
};
use forge_storage::{
    ExecutionStatus, GlobalsRepo, SCRIPT_NAME_KEY, SCRIPT_RUN_NAMED_KIND, ScriptRepo, SettingsRepo,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, VariantKind,
    strip_var_decoration,
};
use time::OffsetDateTime;

use crate::global_mutation::RuntimeScriptGlobals;
use crate::scheduled_runs::SchedulingContext;
use crate::script_registry::{SCRIPT_PANIC_ERROR_TYPE, ScriptRegistry};

const TARGET_VAR_KEY: &str = "target_var";

const SCRIPT: DynamicSelectField<String> =
    DynamicSelectField::new(SCRIPT_NAME_KEY, "Script", SCRIPT_OPTIONS_KEY);

const TARGET_VAR: TextField = TextField {
    key: TARGET_VAR_KEY,
    label: "Output Variable",
    editor: TextEditor::Line {
        placeholder: "script_result",
    },
    default: "",
    required: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SCRIPT, &TARGET_VAR]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String), ConfigProblem> {
    Ok((
        SCRIPT.read(config, args)?,
        strip_var_decoration(&TARGET_VAR.read(config, args)?),
    ))
}

pub struct ScriptRunNamedRunner {
    registry: Arc<ScriptRegistry>,
    globals: Arc<dyn ScriptGlobals>,
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
            globals: Arc::new(RuntimeScriptGlobals::new(globals, Arc::clone(&publisher))),
            publisher,
            settings,
            scripts,
        }
    }
}

#[async_trait]
impl SubActionRunner for ScriptRunNamedRunner {
    fn id(&self) -> &str {
        SCRIPT_RUN_NAMED_KIND
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: TARGET_VAR_KEY.to_owned(),
                kind: VariantKind::String,
                label: "Script result".to_owned(),
            }],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let (name, target_var) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => {
                return (
                    StepTimer::start(ctx, self.id()).invalid_config(&problem),
                    None,
                );
            }
        };
        let started_at = OffsetDateTime::now_utc();
        let wall_start = Instant::now();

        let Some(compiled) = self.registry.get_by_name(&name).await else {
            let duration_ms = wall_start.elapsed().as_millis() as u64;
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: SCRIPT_RUN_NAMED_KIND.to_owned(),
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
        let latest_values = self.registry.latest_values();
        let scheduler = self
            .registry
            .action_scheduler(SchedulingContext::of_run(ctx));
        let parent_event_id = ctx.parent_event_id;
        let integrations = ctx.executor.integration_availability();
        let http_cfg = Arc::new(ScriptHttpConfig::read(self.settings.as_ref()).await);
        let engine_cfg = EngineConfig::read(self.settings.as_ref()).await;

        let exec_event = ScriptStarted {
            script_id: Some(script_id),
            script_name: Some(name.as_str().to_owned()),
            is_inline: false,
        }
        .into_event(EventSource::Rhai, Some(parent_event_id));
        let script_event_id = exec_event.id;
        self.publisher.publish(exec_event);

        let exec_result = tokio::task::spawn_blocking(move || {
            let scope = build_scope_for_contract(&contract, &arg_stack_clone).map_err(|e| {
                ScriptError::Runtime {
                    reason: e.to_string(),
                }
            })?;
            let cfg = engine_cfg;
            let deadline = Instant::now() + Duration::from_millis(cfg.wall_time_ms);
            let http_client =
                ScriptHttpClient::new(http_cfg).map_err(|e| ScriptError::Runtime {
                    reason: e.to_string(),
                })?;
            let mut api = ForgeApi::new(publisher_arc, globals_arc, parent_event_id, deadline)
                .with_http(Arc::new(http_client));
            if let Some(req) = speak_requester {
                api = api.with_speak_requester(req);
            }
            if let Some(integrations) = integrations {
                api = api.with_integration_availability(integrations);
            }
            if let Some(latest) = latest_values {
                api = api.with_latest_values(latest);
            }
            if let Some(scheduler) = scheduler {
                api = api.with_action_scheduler(scheduler);
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
                let message = format!("script '{name}' failed: {script_err}");
                ScriptFailed {
                    script_id: Some(script_id),
                    script_name: Some(name.as_str().to_owned()),
                    is_inline: false,
                    error_type: script_err.error_type().to_owned(),
                    message: message.clone(),
                }
                .publish(
                    self.publisher.as_ref(),
                    EventSource::Rhai,
                    Some(script_event_id),
                );
                (SubActionOutcome::Failed(message), None)
            }
            Err(join_err) => {
                ScriptFailed {
                    script_id: Some(script_id),
                    script_name: Some(name.as_str().to_owned()),
                    is_inline: false,
                    error_type: SCRIPT_PANIC_ERROR_TYPE.to_owned(),
                    message: format!("script task panicked: {join_err}"),
                }
                .publish(
                    self.publisher.as_ref(),
                    EventSource::Rhai,
                    Some(script_event_id),
                );
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
                kind: SCRIPT_RUN_NAMED_KIND.to_owned(),
                started_at,
                duration_ms,
                outcome,
                index: ctx.index,
            },
            updated_stack,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::sandboxed_backend;
    use crate::{EventBus, NullEventLogRepo};
    use forge_events::Event;
    use forge_storage::ScriptRecord;
    use forge_types::{EventId, ScriptContract, ScriptId};
    use time::OffsetDateTime;

    const SECRET: &str = "sk-live-0123456789";

    struct NamedRun {
        outcome: SubActionOutcome,
        stack: Option<ArgStack>,
        exec: Option<Event>,
        error: Option<Event>,
        script_id: ScriptId,
    }

    async fn run_named(body: &str) -> NamedRun {
        run_named_into(body, "").await
    }

    async fn run_named_into(body: &str, target_var: &str) -> NamedRun {
        let backend = sandboxed_backend([0xab; 32]).await.map(Arc::new);
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let publisher: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;

        let script_id = ScriptId::new();
        let ts = OffsetDateTime::now_utc();
        let record = ScriptRecord {
            id: script_id,
            name: "greet".to_owned(),
            body: body.to_owned(),
            contract: ScriptContract::default(),
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
        config.insert(
            TARGET_VAR_KEY.to_owned(),
            Variant::String(target_var.to_owned()),
        );

        let mut sub = bus.subscribe();
        let arg_stack = ArgStack::default();
        let dummy_pub: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
        let ctx =
            forge_registry::RunContext::leaf(&arg_stack, 0, EventId::new(), dummy_pub.as_ref());

        let (telemetry, stack) = runner.execute(&config, &ctx).await;

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
        NamedRun {
            outcome: telemetry.outcome,
            stack,
            exec,
            error,
            script_id,
        }
    }

    #[tokio::test]
    async fn named_script_exec_and_error_carry_script_identity_and_clear_inline_flag() {
        let NamedRun {
            exec,
            error,
            script_id,
            ..
        } = run_named("throw \"boom\";").await;

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

    #[tokio::test]
    async fn named_failure_text_and_error_event_omit_the_script_body() {
        let NamedRun { outcome, error, .. } =
            run_named(&format!("let token = \"{SECRET}\"; let x = 1 / 0;")).await;

        let SubActionOutcome::Failed(message) = outcome else {
            panic!("expected Failed, got {outcome:?}");
        };
        let payload = error
            .expect("failing named run must emit script.error")
            .payload;
        assert!(
            message.starts_with("script 'greet' failed: runtime error:")
                && !message.contains(SECRET)
                && !payload.to_string().contains(SECRET),
            "{message} / {payload}"
        );
    }

    #[tokio::test]
    async fn a_named_script_returning_a_map_stores_it_in_the_target_variable() {
        let NamedRun { outcome, stack, .. } =
            run_named_into(r#"#{name: "ana", tags: [1, 2]}"#, "result").await;

        assert!(matches!(outcome, SubActionOutcome::Success), "{outcome:?}");
        assert_eq!(
            stack.and_then(|s| s.get("result").cloned()),
            Some(Variant::Object(std::collections::BTreeMap::from([
                ("name".to_owned(), Variant::String("ana".to_owned())),
                (
                    "tags".to_owned(),
                    Variant::Array(vec![Variant::Int(1), Variant::Int(2)])
                ),
            ])))
        );
    }

    #[tokio::test]
    async fn a_named_script_returning_nothing_succeeds_without_touching_variables() {
        let NamedRun { outcome, stack, .. } = run_named_into("let x = 1;", "result").await;

        assert!(matches!(outcome, SubActionOutcome::Success), "{outcome:?}");
        assert!(stack.is_none());
    }

    #[tokio::test]
    async fn a_named_script_returning_an_unstorable_value_fails_the_step() {
        let NamedRun { outcome, stack, .. } = run_named_into("[1, ()]", "result").await;

        assert!(
            matches!(&outcome, SubActionOutcome::Failed(m) if m.starts_with("script 'greet' failed:")),
            "{outcome:?}"
        );
        assert!(stack.is_none());
    }
}
