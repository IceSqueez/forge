use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use forge_events::{EventPublisher, EventSource, ScriptFailed, ScriptStarted, TypedEvent};
use forge_registry::{
    CodeField, CodeKind, CodeLanguage, ConfigSpec, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TypedField,
};
use forge_script::{
    Engine, EngineConfig, ForgeApi, ScriptError, ScriptGlobals, ScriptHttpClient, ScriptHttpConfig,
    build_scope_for_contract,
};
use forge_storage::{GlobalsRepo, SettingsRepo};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use crate::core_messages;
use crate::global_mutation::RuntimeScriptGlobals;
use crate::scheduled_runs::SchedulingContext;
use crate::script_registry::{SCRIPT_PANIC_ERROR_TYPE, ScriptRegistry};

const INLINE_SCRIPT_KIND_ID: &str = "script.run.inline";

const BODY: CodeField = CodeField {
    key: "body",
    label: "Script Body",
    language: CodeLanguage::Rhai,
    kind: CodeKind::Script,
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&BODY]);

pub struct ScriptRunInlineRunner {
    registry: Arc<ScriptRegistry>,
    globals: Arc<dyn ScriptGlobals>,
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
            globals: Arc::new(RuntimeScriptGlobals::new(globals, Arc::clone(&publisher))),
            publisher,
            settings,
        }
    }
}

#[async_trait]
impl SubActionRunner for ScriptRunInlineRunner {
    fn id(&self) -> &str {
        INLINE_SCRIPT_KIND_ID
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let body = match BODY.read(config, ctx.arg_stack) {
            Ok(body) => body,
            Err(problem) => {
                return (
                    StepTimer::start(ctx, self.id()).invalid_config(&problem),
                    None,
                );
            }
        };
        let started_at = OffsetDateTime::now_utc();
        let wall_start = Instant::now();

        let publisher_arc = Arc::clone(&self.publisher);
        let globals_arc = Arc::clone(&self.globals);
        let speak_requester = self.registry.speak_requester();
        let latest_values = self.registry.latest_values();
        let scheduler = self
            .registry
            .action_scheduler(SchedulingContext::of_run(ctx));
        let parent_event_id = ctx.parent_event_id;
        let integrations = ctx.executor.integration_availability();
        let arg_stack_clone = ctx.arg_stack.clone();
        let http_cfg = Arc::new(ScriptHttpConfig::read(self.settings.as_ref()).await);
        let engine_cfg = EngineConfig::read(self.settings.as_ref()).await;

        let exec_event = ScriptStarted {
            script_id: None,
            script_name: None,
            is_inline: true,
        }
        .into_event(EventSource::Rhai, Some(parent_event_id));
        let script_event_id = exec_event.id;
        self.publisher.publish(exec_event);

        let exec_result = tokio::task::spawn_blocking(move || {
            let contract = forge_types::ScriptContract::default();
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
            engine.eval_script_with_scope(&body, &mut scope)
        })
        .await;

        let duration_ms = wall_start.elapsed().as_millis() as u64;

        let outcome = match exec_result {
            Ok(Ok(_)) => SubActionOutcome::Success,
            Ok(Err(script_err)) => {
                let message = format!("inline script failed: {script_err}");
                ScriptFailed {
                    script_id: None,
                    script_name: None,
                    is_inline: true,
                    error_type: script_err.error_type().to_owned(),
                    message: message.clone(),
                }
                .publish(
                    self.publisher.as_ref(),
                    EventSource::Rhai,
                    Some(script_event_id),
                );
                SubActionOutcome::Failed(core_messages::inline_script_failed(&script_err).into())
            }
            Err(join_err) => {
                ScriptFailed {
                    script_id: None,
                    script_name: None,
                    is_inline: true,
                    error_type: SCRIPT_PANIC_ERROR_TYPE.to_owned(),
                    message: format!("script task panicked: {join_err}"),
                }
                .publish(
                    self.publisher.as_ref(),
                    EventSource::Rhai,
                    Some(script_event_id),
                );
                SubActionOutcome::Failed(core_messages::script_panicked(join_err).into())
            }
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: INLINE_SCRIPT_KIND_ID.to_owned(),
                started_at,
                duration_ms,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Sandboxed, sandboxed_backend};
    use crate::{EventBus, NullEventLogRepo, ScriptRegistry};
    use forge_events::Event;
    use forge_events::EventPublisher;
    use forge_storage::{GlobalsRepo, SettingsRepo};
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::EventId;
    use forge_types::Variant;

    async fn make_backend() -> Sandboxed<Arc<SqliteBackend>> {
        let backend = sandboxed_backend([0xab; 32]).await.map(Arc::new);
        backend
            .set_string(forge_storage::reserved_keys::SCRIPT_TIMEOUT_MS, "30000")
            .await
            .unwrap();
        backend
            .set_string(forge_storage::reserved_keys::SCRIPT_OP_LIMIT, "500")
            .await
            .unwrap();
        backend
    }

    async fn run_inline(body: &str) -> (SubActionOutcome, Option<Event>, Option<Event>) {
        run_inline_on(make_backend().await, body).await
    }

    async fn run_inline_on(
        backend: Sandboxed<Arc<SqliteBackend>>,
        body: &str,
    ) -> (SubActionOutcome, Option<Event>, Option<Event>) {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let publisher: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
        let runner = ScriptRunInlineRunner::new(
            Arc::new(ScriptRegistry::new()),
            Arc::clone(&backend) as Arc<dyn GlobalsRepo>,
            publisher,
            Arc::clone(&backend) as Arc<dyn SettingsRepo>,
        );

        let mut config = runner.default_config();
        config.insert("body".to_owned(), Variant::String(body.to_owned()));

        let mut sub = bus.subscribe();
        let arg_stack = ArgStack::default();
        let dummy_pub: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
        let ctx =
            forge_registry::RunContext::leaf(&arg_stack, 0, EventId::new(), dummy_pub.as_ref());

        let (telemetry, _) = runner.execute(&config, &ctx).await;

        let mut exec = None;
        let mut error = None;
        for _ in 0..40 {
            match tokio::time::timeout(std::time::Duration::from_millis(50), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == "script.exec" => exec = Some(ev),
                Ok(Ok(ev)) if ev.kind == "script.error" => error = Some(ev),
                Ok(Ok(_)) => {}
                _ => break,
            }
            if exec.is_some() && error.is_some() {
                break;
            }
        }
        (telemetry.outcome, exec, error)
    }

    #[tokio::test]
    async fn http_module_wired_denied_domain_returns_failed() {
        let (outcome, _, _) = run_inline(r#"forge::http::get("https://nope.invalid")"#).await;

        let SubActionOutcome::Failed(msg) = outcome else {
            panic!("expected Failed, got {outcome:?}");
        };
        assert!(
            msg.code_or_text().contains("domain not allowed"),
            "expected domain-not-allowed error from wired http module, got: {msg:?}"
        );
    }

    #[tokio::test]
    async fn inline_script_http_call_runs_inside_the_runtime_and_reaches_an_allowlisted_server() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::any())
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("pong"))
            .mount(&server)
            .await;
        let backend = make_backend().await;
        backend
            .set_string(
                forge_storage::reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS,
                &format!(r#"["{}"]"#, server.address().ip()),
            )
            .await
            .unwrap();
        backend
            .set_string(
                forge_storage::reserved_keys::SCRIPT_HTTP_ALLOW_LOCAL,
                "true",
            )
            .await
            .unwrap();
        let body = format!(
            r#"let r = forge::http::get("{}/ping"); if r.body != "pong" {{ throw "bad body"; }}"#,
            server.uri()
        );

        let (outcome, _, error) = run_inline_on(backend, &body).await;

        assert_eq!(outcome, SubActionOutcome::Success, "{error:?}");
    }

    #[tokio::test]
    async fn inline_script_exec_and_error_carry_null_identity_and_inline_flag() {
        let (_, exec, error) = run_inline("throw \"boom\";").await;

        for ev in [
            exec.expect("inline run must emit script.exec"),
            error.expect("failing inline run must emit script.error"),
        ] {
            assert!(
                ev.payload["script_id"].is_null(),
                "inline {} must carry a null script_id",
                ev.kind
            );
            assert!(
                ev.payload["script_name"].is_null(),
                "inline {} must carry a null script_name",
                ev.kind
            );
            assert_eq!(
                ev.payload["is_inline"].as_bool(),
                Some(true),
                "inline {} must flag is_inline=true",
                ev.kind
            );
        }
    }

    #[tokio::test]
    async fn inline_failure_text_and_error_event_omit_the_script_body() {
        const SECRET: &str = "sk-live-0123456789";
        for (body, error_type) in [
            (
                format!("let token = \"{SECRET}\"; let x = 1 / 0;"),
                "runtime",
            ),
            (format!("let token = \"{SECRET}\"; @@@"), "syntax"),
            (
                format!("let token = \"{SECRET}\"; loop {{}}"),
                "ops_exceeded",
            ),
        ] {
            let (outcome, _, error) = run_inline(&body).await;
            let SubActionOutcome::Failed(message) = outcome else {
                panic!("expected Failed for {error_type}, got {outcome:?}");
            };
            let payload = error
                .expect("failing inline run must emit script.error")
                .payload;
            assert_eq!(payload["error_type"].as_str(), Some(error_type));
            assert!(
                message.code_or_text().starts_with("inline script failed:")
                    && !message.code_or_text().contains(SECRET)
                    && !payload.to_string().contains(SECRET),
                "{error_type}: {message:?} / {payload}"
            );
        }
    }
}
