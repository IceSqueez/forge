use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct HotkeyTriggerRunner {
    sink: Arc<dyn VTubeSink>,
}

impl HotkeyTriggerRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for HotkeyTriggerRunner {
    fn id(&self) -> &str {
        "vtube.hotkey.trigger"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Trigger Hotkey"
    }

    fn summary(&self) -> &str {
        "Triggers a VTube Studio hotkey by its ID."
    }

    fn search_text(&self) -> &str {
        "vtube hotkey trigger animation expression vts"
    }

    fn icon_name(&self) -> &str {
        "bolt"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("hotkey_id".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "hotkey_id",
            label: "Hotkey",
            options_key: "vtube.hotkey_ids",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("hotkey_id") {
            Some(Variant::String(_)) => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "vtube.hotkey.trigger: 'hotkey_id' must be a string".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw = config
            .get("hotkey_id")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let hotkey_id = ctx.arg_stack.interpolate(raw);

        let outcome = match self.sink.trigger_hotkey(&hotkey_id).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "vtube.hotkey.trigger".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::error::VTubeError;
    use crate::runners::test_support::{MockSink, make_ctx};

    #[test]
    fn validate_config_accepts_hotkey_string() {
        let runner = HotkeyTriggerRunner::new(Arc::new(MockSink::new()));
        let config = BTreeMap::from([("hotkey_id".to_owned(), Variant::String("hk-1".to_owned()))]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_missing_hotkey_id() {
        let runner = HotkeyTriggerRunner::new(Arc::new(MockSink::new()));
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }

    #[tokio::test]
    async fn execute_interpolates_hotkey_id() {
        let runner = HotkeyTriggerRunner::new(Arc::new(MockSink::new()));
        let stack = ArgStack::new().set("id".to_owned(), Variant::String("hk-abc".to_owned()));
        let config = BTreeMap::from([("hotkey_id".to_owned(), Variant::String("%id%".to_owned()))]);
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(extra.is_none());
    }

    #[tokio::test]
    async fn execute_returns_success_on_mock_sink() {
        let runner = HotkeyTriggerRunner::new(Arc::new(MockSink::new()));
        let stack = ArgStack::new();
        let config =
            BTreeMap::from([("hotkey_id".to_owned(), Variant::String("hk-xyz".to_owned()))]);
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "vtube.hotkey.trigger");
    }

    struct CaptureSink {
        last_id: Arc<std::sync::Mutex<Option<String>>>,
    }

    impl CaptureSink {
        fn new() -> Self {
            Self {
                last_id: Arc::new(std::sync::Mutex::new(None)),
            }
        }

        fn captured_id(&self) -> Option<String> {
            self.last_id.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl VTubeSink for CaptureSink {
        async fn trigger_hotkey(&self, hotkey_id: &str) -> Result<(), VTubeError> {
            *self.last_id.lock().unwrap() = Some(hotkey_id.to_owned());
            Ok(())
        }
        async fn set_expression(&self, _: &str, _: bool) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn set_param(&self, _: &str, _: f64) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn load_model(&self, _: &str) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn reset_params(&self) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn move_model(
            &self,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: f64,
        ) -> Result<(), VTubeError> {
            Ok(())
        }
        #[allow(clippy::too_many_arguments)]
        async fn move_item(
            &self,
            _: &str,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<i64>,
            _: f64,
            _: &str,
        ) -> Result<(), VTubeError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn execute_passes_interpolated_id_to_sink() {
        let sink = Arc::new(CaptureSink::new());
        let runner = HotkeyTriggerRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new().set("id".to_owned(), Variant::String("hk-abc".to_owned()));
        let config = BTreeMap::from([("hotkey_id".to_owned(), Variant::String("%id%".to_owned()))]);
        let ctx = make_ctx(&stack);
        runner.execute(&config, &ctx).await;
        assert_eq!(
            sink.captured_id().as_deref(),
            Some("hk-abc"),
            "sink must receive the interpolated hotkey ID, not the raw template"
        );
    }

    #[tokio::test]
    async fn execute_propagates_sink_error_as_failed_outcome() {
        let runner = HotkeyTriggerRunner::new(Arc::new(MockSink::failing()));
        let stack = ArgStack::new();
        let config =
            BTreeMap::from([("hotkey_id".to_owned(), Variant::String("hk-bad".to_owned()))]);
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert!(
            matches!(tel.outcome, SubActionOutcome::Failed(_)),
            "sink error must produce Failed outcome"
        );
    }
}
