use crate::error::ScriptError;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use forge_events::EventPublisher;
use forge_storage::SettingsRepo;
use forge_types::{
    ArgStack, EventId, IntegrationAvailability, LatestValueReader, ScriptContract, ScriptId,
    Variant,
};

use crate::contract::build_scope_for_contract;
use crate::globals_port::ScriptGlobals;
use crate::schedule::ActionScheduler;
use crate::{Engine, EngineConfig, ForgeApi, ScriptHttpClient, ScriptHttpConfig, SpeakRequester};

#[derive(Debug, Clone)]
pub struct RunResult {
    pub script_id: ScriptId,
    pub duration_ms: f64,
    pub error_count: usize,
    pub output_display: Option<String>,
}

#[derive(Default)]
pub struct ScriptCapabilities {
    pub speak: Option<Arc<dyn SpeakRequester>>,
    pub integrations: Option<Arc<dyn IntegrationAvailability>>,
    pub latest_values: Option<Arc<dyn LatestValueReader>>,
    pub scheduler: Option<Arc<dyn ActionScheduler>>,
}

pub struct ScriptSession {
    publisher: Arc<dyn EventPublisher>,
    globals: Arc<dyn ScriptGlobals>,
    caused_by: EventId,
    script_id: Option<ScriptId>,
    engine_config: EngineConfig,
    http_config: ScriptHttpConfig,
    capabilities: ScriptCapabilities,
}

impl ScriptSession {
    pub async fn open(
        settings: &dyn SettingsRepo,
        publisher: Arc<dyn EventPublisher>,
        globals: Arc<dyn ScriptGlobals>,
        caused_by: EventId,
        capabilities: ScriptCapabilities,
    ) -> Self {
        Self {
            publisher,
            globals,
            caused_by,
            script_id: None,
            engine_config: EngineConfig::read(settings).await,
            http_config: ScriptHttpConfig::read(settings).await,
            capabilities,
        }
    }

    fn for_script(mut self, script_id: ScriptId) -> Self {
        self.script_id = Some(script_id);
        self
    }

    pub async fn run_for_value(
        self,
        body: String,
        contract: ScriptContract,
        args: ArgStack,
    ) -> Result<Result<Option<Variant>, ScriptError>, ScriptTaskPanicked> {
        tokio::task::spawn_blocking(move || {
            let mut scope = build_scope_for_contract(&contract, &args)?;
            self.into_engine()?
                .eval_script_with_scope_as_variant(&body, &mut scope)
        })
        .await
        .map_err(ScriptTaskPanicked::from)
    }

    pub async fn run_discarding_value(
        self,
        body: String,
        args: ArgStack,
    ) -> Result<Result<(), ScriptError>, ScriptTaskPanicked> {
        tokio::task::spawn_blocking(move || {
            let mut scope = build_scope_for_contract(&ScriptContract::default(), &args)?;
            self.into_engine()?
                .eval_script_with_scope(&body, &mut scope)
                .map(drop)
        })
        .await
        .map_err(ScriptTaskPanicked::from)
    }

    pub(crate) fn into_engine(self) -> Result<Engine, ScriptError> {
        self.assemble().map(|(engine, _)| engine)
    }

    fn assemble(self) -> Result<(Engine, Arc<AtomicU32>), ScriptError> {
        let Self {
            publisher,
            globals,
            caused_by,
            script_id,
            engine_config,
            http_config,
            capabilities,
        } = self;
        let deadline = Instant::now() + Duration::from_millis(engine_config.wall_time_ms);
        let http =
            ScriptHttpClient::new(Arc::new(http_config)).map_err(|e| ScriptError::Runtime {
                reason: e.to_string(),
            })?;
        let mut api =
            ForgeApi::new(publisher, globals, caused_by, deadline).with_http(Arc::new(http));
        if let Some(script_id) = script_id {
            api = api.with_script_id(script_id);
        }
        if let Some(requester) = capabilities.speak {
            api = api.with_speak_requester(requester);
        }
        if let Some(integrations) = capabilities.integrations {
            api = api.with_integration_availability(integrations);
        }
        if let Some(latest) = capabilities.latest_values {
            api = api.with_latest_values(latest);
        }
        if let Some(scheduler) = capabilities.scheduler {
            api = api.with_action_scheduler(scheduler);
        }
        let error_count = api.error_count_handle();
        Ok((Engine::with_api(engine_config, api), error_count))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ScriptTaskPanicked(String);

impl From<tokio::task::JoinError> for ScriptTaskPanicked {
    fn from(error: tokio::task::JoinError) -> Self {
        Self(error.to_string())
    }
}

pub struct ScriptHost {
    pub globals: Arc<dyn ScriptGlobals>,
    pub settings: Arc<dyn SettingsRepo>,
    pub bus: Arc<dyn EventPublisher>,
    pub capabilities: ScriptCapabilities,
}

pub async fn run_inline(
    body: String,
    contract: ScriptContract,
    arg_stack: ArgStack,
    host: ScriptHost,
    script_id: ScriptId,
) -> Result<RunResult, ScriptError> {
    let ScriptHost {
        globals,
        settings,
        bus,
        capabilities,
    } = host;
    let mut scope = build_scope_for_contract(&contract, &arg_stack)?;
    let session = ScriptSession::open(
        settings.as_ref(),
        bus,
        globals,
        EventId::new(),
        capabilities,
    )
    .await
    .for_script(script_id);
    let start = Instant::now();
    let (result, error_count) = tokio::task::spawn_blocking(move || {
        let (engine, error_count) = session.assemble()?;
        let result = engine.eval_script_with_scope(&body, &mut scope);
        Ok::<_, ScriptError>((result, error_count))
    })
    .await
    .map_err(|e| ScriptError::Runtime {
        reason: e.to_string(),
    })??;
    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    let returned = result?;
    let output_display = (!returned.is_unit()).then(|| returned.to_string());
    Ok(RunResult {
        script_id,
        duration_ms,
        error_count: error_count.load(Ordering::Relaxed) as usize,
        output_display,
    })
}

#[cfg(test)]
mod tests {
    use forge_storage::DataProvider;
    use std::sync::Mutex;

    use forge_types::IntegrationId;

    use super::*;

    use crate::test_support::{host_over, repo_globals, sandboxed_backend};
    use forge_events::test_support::CapturingPublisher;

    struct TwitchDisabled;

    impl IntegrationAvailability for TwitchDisabled {
        fn is_disabled(&self, integration: &IntegrationId) -> bool {
            integration.as_str() == "twitch"
        }
    }

    #[tokio::test]
    async fn an_editor_run_sending_to_a_disabled_integration_fails_and_publishes_no_send() {
        let backend = sandboxed_backend().await.map(Arc::new);
        let captured = CapturingPublisher::shared();
        let host = host_over(
            &backend,
            Arc::clone(&captured) as Arc<dyn EventPublisher>,
            ScriptCapabilities {
                integrations: Some(Arc::new(TwitchDisabled)),
                ..ScriptCapabilities::default()
            },
        );

        let result = run_inline(
            r#"forge::chat::send("twitch", "hello")"#.to_owned(),
            ScriptContract::default(),
            ArgStack::new(),
            host,
            ScriptId::new(),
        )
        .await;

        match result {
            Err(ScriptError::Runtime { reason, .. }) => assert!(
                reason.contains(&format!(
                    "{} (integration=twitch)",
                    forge_types::INTEGRATION_DISABLED
                )),
                "the run must fail naming the disabled integration, got: {reason}"
            ),
            other => panic!("expected a runtime error, got {other:?}"),
        }
        assert!(
            !captured
                .events()
                .iter()
                .any(|e| e.kind == "chat.send.request"),
            "no chat.send.request may leave a run aimed at a disabled integration"
        );
    }

    struct DonationReader;

    impl LatestValueReader for DonationReader {
        fn latest(
            &self,
            slot: &str,
            _scope: forge_types::LatestScope<'_>,
        ) -> Option<forge_types::LatestValue> {
            (slot == "donation").then(|| {
                forge_types::LatestValue::new(
                    "donatello",
                    time::OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap(),
                    std::collections::BTreeMap::from([(
                        "marker".to_owned(),
                        forge_types::Variant::String("from-the-reader".to_owned()),
                    )]),
                )
            })
        }
    }

    async fn run_latest_probe(
        latest_values: Option<Arc<dyn LatestValueReader>>,
    ) -> Result<RunResult, ScriptError> {
        let backend = sandboxed_backend().await.map(Arc::new);
        let host = host_over(
            &backend,
            CapturingPublisher::shared(),
            ScriptCapabilities {
                latest_values,
                ..ScriptCapabilities::default()
            },
        );
        run_inline(
            r#"forge::latest::get("donation").marker"#.to_owned(),
            ScriptContract::default(),
            ArgStack::new(),
            host,
            ScriptId::new(),
        )
        .await
    }

    #[tokio::test]
    async fn an_editor_run_reads_the_latest_value_from_the_wired_reader() {
        let result = run_latest_probe(Some(Arc::new(DonationReader)))
            .await
            .unwrap();

        assert_eq!(result.output_display.as_deref(), Some("from-the-reader"));
    }

    #[tokio::test]
    async fn an_editor_run_without_a_reader_fails_saying_latest_values_are_unavailable() {
        match run_latest_probe(None).await {
            Err(ScriptError::Runtime { reason, .. }) => assert!(
                reason.contains("latest values are not available where this script runs"),
                "got: {reason}"
            ),
            other => panic!("expected the latest-unavailable runtime error, got {other:?}"),
        }
    }

    struct KeyHolder;

    #[async_trait::async_trait]
    impl ActionScheduler for KeyHolder {
        async fn schedule(
            &self,
            _request: crate::ScriptScheduleRequest,
        ) -> Result<crate::ScriptSchedulePlacement, crate::ScriptError> {
            Err(crate::ScriptError::ScheduleRejected(
                "not under test".to_owned(),
            ))
        }

        async fn cancel_by_key(&self, key: &str) -> Result<bool, crate::ScriptError> {
            Ok(key == "raid-thanks")
        }
    }

    #[tokio::test]
    async fn an_editor_run_schedules_through_the_wired_scheduler() {
        let backend = sandboxed_backend().await.map(Arc::new);
        let host = host_over(
            &backend,
            CapturingPublisher::shared(),
            ScriptCapabilities {
                scheduler: Some(Arc::new(KeyHolder)),
                ..ScriptCapabilities::default()
            },
        );

        let result = run_inline(
            r#"forge::schedule::cancel("raid-thanks")"#.to_owned(),
            ScriptContract::default(),
            ArgStack::new(),
            host,
            ScriptId::new(),
        )
        .await
        .unwrap();

        assert_eq!(result.output_display.as_deref(), Some("true"));
    }

    struct RecordingSpeaker(Mutex<Vec<String>>);

    #[async_trait::async_trait]
    impl SpeakRequester for RecordingSpeaker {
        async fn speak(
            &self,
            text: String,
            _voice: Option<String>,
        ) -> Result<(), crate::ScriptError> {
            self.0.lock().unwrap().push(text);
            Ok(())
        }

        async fn skip(&self) {}

        async fn clear(&self) {}
    }

    #[tokio::test]
    async fn an_editor_run_speaks_through_the_wired_requester() {
        let backend = sandboxed_backend().await.map(Arc::new);
        let speaker = Arc::new(RecordingSpeaker(Mutex::new(Vec::new())));
        let host = host_over(
            &backend,
            CapturingPublisher::shared(),
            ScriptCapabilities {
                speak: Some(Arc::clone(&speaker) as Arc<dyn SpeakRequester>),
                ..ScriptCapabilities::default()
            },
        );

        run_inline(
            r#"forge::tts::speak("hello")"#.to_owned(),
            ScriptContract::default(),
            ArgStack::new(),
            host,
            ScriptId::new(),
        )
        .await
        .unwrap();

        assert_eq!(*speaker.0.lock().unwrap(), ["hello"]);
    }

    #[tokio::test]
    async fn an_editor_run_reaches_the_http_client_and_its_deny_by_default_policy() {
        let backend = sandboxed_backend().await.map(Arc::new);
        let host = host_over(
            &backend,
            CapturingPublisher::shared(),
            ScriptCapabilities::default(),
        );

        let result = run_inline(
            r#"forge::http::get("https://192.0.2.1/")"#.to_owned(),
            ScriptContract::default(),
            ArgStack::new(),
            host,
            ScriptId::new(),
        )
        .await;

        match result {
            Err(ScriptError::Runtime { reason, .. }) => assert!(
                !reason.contains("not available where this script runs"),
                "the run must reach the client, got: {reason}"
            ),
            other => panic!("expected the policy to refuse the request, got {other:?}"),
        }
    }

    async fn open_session(
        store: &forge_storage::test_support::MemorySettings,
        bus: Arc<dyn EventPublisher>,
        caused_by: EventId,
    ) -> Engine {
        let backend = sandboxed_backend().await.map(Arc::new);
        ScriptSession::open(
            store,
            bus,
            repo_globals(backend.globals_repo()),
            caused_by,
            ScriptCapabilities::default(),
        )
        .await
        .into_engine()
        .unwrap()
    }

    #[tokio::test]
    async fn a_session_runs_under_the_stored_limits() {
        let store = forge_storage::test_support::MemorySettings::new();
        store.seed_raw(&crate::settings::OP_LIMIT, "7777");
        let bus = CapturingPublisher::shared();
        let engine = open_session(&store, bus, EventId::new()).await;

        let error = tokio::task::spawn_blocking(move || engine.eval_script("loop {}"))
            .await
            .unwrap()
            .unwrap_err();

        assert!(
            matches!(error, ScriptError::OperationLimit { ops: 7777 }),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn a_session_attributes_what_the_script_publishes_to_its_cause() {
        let captured = CapturingPublisher::shared();
        let cause = EventId::new();
        let store = forge_storage::test_support::MemorySettings::new();
        let bus = Arc::clone(&captured);
        let engine = open_session(&store, bus, cause).await;

        let sent = tokio::task::spawn_blocking(move || {
            engine.eval_script(r#"forge::chat::send("twitch", "hi")"#)
        })
        .await
        .unwrap();

        assert!(sent.is_ok(), "{sent:?}");

        let events = captured.events();
        assert!(
            events
                .iter()
                .any(|event| event.kind == "chat.send.request" && event.caused_by == Some(cause)),
            "{:?}",
            events.iter().map(|event| &event.kind).collect::<Vec<_>>()
        );
    }

    async fn session_for_value() -> ScriptSession {
        let store = forge_storage::test_support::MemorySettings::new();
        let backend = sandboxed_backend().await.map(Arc::new);
        ScriptSession::open(
            &store,
            CapturingPublisher::shared() as Arc<dyn EventPublisher>,
            repo_globals(backend.globals_repo()),
            EventId::new(),
            ScriptCapabilities::default(),
        )
        .await
    }

    fn contract_with_input(name: &str, kind: forge_types::VariantKind) -> ScriptContract {
        ScriptContract {
            inputs: vec![forge_types::ScriptInput {
                name: name.to_owned(),
                kind,
            }],
            returns: None,
        }
    }

    #[tokio::test]
    async fn run_for_value_returns_the_scripts_value_as_a_variant() {
        let value = session_for_value()
            .await
            .run_for_value(
                "40 + 2".to_owned(),
                ScriptContract::default(),
                ArgStack::new(),
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(value, Some(Variant::Int(42)));
    }

    #[tokio::test]
    async fn run_for_value_returns_none_for_a_script_without_a_value() {
        let value = session_for_value()
            .await
            .run_for_value(
                "let ignored = 1;".to_owned(),
                ScriptContract::default(),
                ArgStack::new(),
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn run_for_value_exposes_declared_inputs_to_the_script() {
        let args = ArgStack::new().set("bits".to_owned(), Variant::Int(20));

        let value = session_for_value()
            .await
            .run_for_value(
                "bits * 2".to_owned(),
                contract_with_input("bits", forge_types::VariantKind::Int),
                args,
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(value, Some(Variant::Int(40)));
    }

    #[tokio::test]
    async fn run_for_value_reports_a_missing_declared_input() {
        let outcome = session_for_value()
            .await
            .run_for_value(
                "bits".to_owned(),
                contract_with_input("bits", forge_types::VariantKind::Int),
                ArgStack::new(),
            )
            .await
            .unwrap();

        assert!(
            matches!(&outcome, Err(ScriptError::MissingInput { name }) if name == "bits"),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn run_for_value_reports_an_input_of_the_wrong_kind() {
        let args = ArgStack::new().set("bits".to_owned(), Variant::String("many".to_owned()));

        let outcome = session_for_value()
            .await
            .run_for_value(
                "bits".to_owned(),
                contract_with_input("bits", forge_types::VariantKind::Int),
                args,
            )
            .await
            .unwrap();

        assert!(
            matches!(
                &outcome,
                Err(ScriptError::InputTypeMismatch {
                    expected: forge_types::VariantKind::Int,
                    got: forge_types::VariantKind::String,
                    ..
                })
            ),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn run_for_value_reports_a_script_that_does_not_compile() {
        let outcome = session_for_value()
            .await
            .run_for_value(
                "let = ;".to_owned(),
                ScriptContract::default(),
                ArgStack::new(),
            )
            .await
            .unwrap();

        assert!(outcome.is_err(), "{outcome:?}");
    }
}
