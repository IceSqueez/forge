use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use forge_events::EventPublisher;
use forge_storage::SettingsRepo;
use forge_types::{
    ArgStack, EventId, IntegrationAvailability, LatestValueReader, ScriptContract, ScriptId,
};

use crate::error::ScriptError;
use crate::globals_port::ScriptGlobals;
use crate::schedule::ActionScheduler;
use crate::{
    Engine, EngineConfig, ForgeApi, ScriptHttpClient, ScriptHttpConfig, SpeakRequester,
    build_scope_for_contract,
};

#[derive(Debug, Clone)]
pub struct RunResult {
    pub script_id: ScriptId,
    pub duration_ms: f64,
    pub error_count: usize,
    pub output_display: String,
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

    pub fn into_engine(self) -> Result<Engine, ScriptError> {
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
    let mut scope =
        build_scope_for_contract(&contract, &arg_stack).map_err(|e| ScriptError::Runtime {
            reason: e.to_string(),
        })?;
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
    let output_display = match result {
        Ok(dyn_val) => {
            if dyn_val.is_unit() {
                "(unit)".to_string()
            } else {
                dyn_val.to_string()
            }
        }
        Err(e) => return Err(e),
    };
    Ok(RunResult {
        script_id,
        duration_ms,
        error_count: error_count.load(Ordering::Relaxed) as usize,
        output_display,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use forge_events::Event;
    use forge_types::IntegrationId;

    use super::*;
    use crate::test_support::{repo_globals, sandboxed_backend};

    struct CapturingPublisher(Arc<Mutex<Vec<Event>>>);

    impl EventPublisher for CapturingPublisher {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    struct TwitchDisabled;

    impl IntegrationAvailability for TwitchDisabled {
        fn is_disabled(&self, integration: &IntegrationId) -> bool {
            integration.as_str() == "twitch"
        }
    }

    #[tokio::test]
    async fn an_editor_run_sending_to_a_disabled_integration_fails_and_publishes_no_send() {
        let backend = sandboxed_backend([0xab; 32]).await.map(Arc::new);
        let captured = Arc::new(Mutex::new(Vec::new()));
        let host = ScriptHost {
            globals: repo_globals(Arc::clone(&backend)),
            settings: Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            bus: Arc::new(CapturingPublisher(Arc::clone(&captured))),
            capabilities: ScriptCapabilities {
                integrations: Some(Arc::new(TwitchDisabled)),
                ..ScriptCapabilities::default()
            },
        };

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
                .lock()
                .unwrap()
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
        let backend = sandboxed_backend([0xab; 32]).await.map(Arc::new);
        let host = ScriptHost {
            globals: repo_globals(Arc::clone(&backend)),
            settings: Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            bus: Arc::new(CapturingPublisher(Arc::new(Mutex::new(Vec::new())))),
            capabilities: ScriptCapabilities {
                latest_values,
                ..ScriptCapabilities::default()
            },
        };
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

        assert_eq!(result.output_display, "from-the-reader");
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
        ) -> Result<crate::ScriptSchedulePlacement, crate::ScriptScheduleError> {
            Err(crate::ScriptScheduleError::Rejected(
                "not under test".to_owned(),
            ))
        }

        async fn cancel_by_key(&self, key: &str) -> Result<bool, crate::ScriptScheduleError> {
            Ok(key == "raid-thanks")
        }
    }

    #[tokio::test]
    async fn an_editor_run_schedules_through_the_wired_scheduler() {
        let backend = sandboxed_backend([0xab; 32]).await.map(Arc::new);
        let host = ScriptHost {
            globals: repo_globals(Arc::clone(&backend)),
            settings: Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            bus: Arc::new(CapturingPublisher(Arc::new(Mutex::new(Vec::new())))),
            capabilities: ScriptCapabilities {
                scheduler: Some(Arc::new(KeyHolder)),
                ..ScriptCapabilities::default()
            },
        };

        let result = run_inline(
            r#"forge::schedule::cancel("raid-thanks")"#.to_owned(),
            ScriptContract::default(),
            ArgStack::new(),
            host,
            ScriptId::new(),
        )
        .await
        .unwrap();

        assert_eq!(result.output_display, "true");
    }

    struct RecordingSpeaker(Mutex<Vec<String>>);

    #[async_trait::async_trait]
    impl SpeakRequester for RecordingSpeaker {
        async fn speak(
            &self,
            text: String,
            _voice: Option<String>,
        ) -> Result<(), crate::ScriptSpeakError> {
            self.0.lock().unwrap().push(text);
            Ok(())
        }

        async fn skip(&self) {}

        async fn clear(&self) {}
    }

    #[tokio::test]
    async fn an_editor_run_speaks_through_the_wired_requester() {
        let backend = sandboxed_backend([0xab; 32]).await.map(Arc::new);
        let speaker = Arc::new(RecordingSpeaker(Mutex::new(Vec::new())));
        let host = ScriptHost {
            globals: repo_globals(Arc::clone(&backend)),
            settings: Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            bus: Arc::new(CapturingPublisher(Arc::new(Mutex::new(Vec::new())))),
            capabilities: ScriptCapabilities {
                speak: Some(Arc::clone(&speaker) as Arc<dyn SpeakRequester>),
                ..ScriptCapabilities::default()
            },
        };

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
        let backend = sandboxed_backend([0xab; 32]).await.map(Arc::new);
        let host = ScriptHost {
            globals: repo_globals(Arc::clone(&backend)),
            settings: Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            bus: Arc::new(CapturingPublisher(Arc::new(Mutex::new(Vec::new())))),
            capabilities: ScriptCapabilities::default(),
        };

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
        let backend = sandboxed_backend([0xab; 32]).await.map(Arc::new);
        ScriptSession::open(
            store,
            bus,
            repo_globals(Arc::clone(&backend)),
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
        let bus = Arc::new(CapturingPublisher(Arc::new(Mutex::new(Vec::new()))));
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
        let captured = Arc::new(Mutex::new(Vec::new()));
        let cause = EventId::new();
        let store = forge_storage::test_support::MemorySettings::new();
        let bus = Arc::new(CapturingPublisher(Arc::clone(&captured)));
        let engine = open_session(&store, bus, cause).await;

        let sent = tokio::task::spawn_blocking(move || {
            engine.eval_script(r#"forge::chat::send("twitch", "hi")"#)
        })
        .await
        .unwrap();

        assert!(sent.is_ok(), "{sent:?}");

        let events = captured.lock().unwrap();
        assert!(
            events
                .iter()
                .any(|event| event.kind == "chat.send.request" && event.caused_by == Some(cause)),
            "{:?}",
            events.iter().map(|event| &event.kind).collect::<Vec<_>>()
        );
    }
}
