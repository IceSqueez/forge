use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use forge_events::EventPublisher;
use forge_storage::{GlobalsRepo, SettingsRepo};
use forge_types::{
    ArgStack, EventId, IntegrationAvailability, LatestValueReader, ScriptContract, ScriptId,
};

use crate::error::ScriptError;
use crate::{Engine, ForgeApi, build_scope_for_contract, load_script_engine_config};

#[derive(Debug, Clone)]
pub struct RunResult {
    pub script_id: ScriptId,
    pub duration_ms: f64,
    pub error_count: usize,
    pub output_display: String,
}

pub fn content_hash(body: &str) -> String {
    let mut h = DefaultHasher::new();
    body.hash(&mut h);
    format!("{:016x}", h.finish())
}

pub struct ScriptHost {
    pub globals: Arc<dyn GlobalsRepo>,
    pub settings: Arc<dyn SettingsRepo>,
    pub bus: Arc<dyn EventPublisher>,
    pub integrations: Arc<dyn IntegrationAvailability>,
    pub latest_values: Option<Arc<dyn LatestValueReader>>,
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
        integrations,
        latest_values,
    } = host;
    let mut scope =
        build_scope_for_contract(&contract, &arg_stack).map_err(|e| ScriptError::Runtime {
            script: body.chars().take(80).collect(),
            reason: e.to_string(),
        })?;
    let cfg = load_script_engine_config(settings.as_ref()).await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(cfg.wall_time_ms);
    let mut api = ForgeApi::new(bus, globals, EventId::new(), deadline)
        .with_script_id(script_id)
        .with_integration_availability(integrations);
    if let Some(reader) = latest_values {
        api = api.with_latest_values(reader);
    }
    let error_count = api.error_count_handle();
    let engine = Engine::with_api(cfg, api);
    let start = std::time::Instant::now();
    let result =
        tokio::task::spawn_blocking(move || engine.eval_script_with_scope(&body, &mut scope))
            .await
            .map_err(|e| ScriptError::Runtime {
                script: String::new(),
                reason: e.to_string(),
            })?;
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
        error_count: error_count.load(std::sync::atomic::Ordering::Relaxed) as usize,
        output_display,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::Mutex;

    use forge_events::Event;
    use forge_types::IntegrationId;

    use super::*;
    use crate::test_support::sandboxed_backend;

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
            globals: Arc::clone(&backend) as Arc<dyn GlobalsRepo>,
            settings: Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            bus: Arc::new(CapturingPublisher(Arc::clone(&captured))),
            integrations: Arc::new(TwitchDisabled),
            latest_values: None,
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
                reason.contains("integration disabled: twitch"),
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
            globals: Arc::clone(&backend) as Arc<dyn GlobalsRepo>,
            settings: Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            bus: Arc::new(CapturingPublisher(Arc::new(Mutex::new(Vec::new())))),
            integrations: Arc::new(TwitchDisabled),
            latest_values,
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
    async fn an_editor_run_without_a_reader_has_no_latest_namespace() {
        match run_latest_probe(None).await {
            Err(ScriptError::Runtime { reason, .. }) => {
                assert!(reason.contains("forge::latest::get"), "got: {reason}")
            }
            other => panic!("expected a missing-function error, got {other:?}"),
        }
    }
}
