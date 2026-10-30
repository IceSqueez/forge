use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, PickedTextField, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextPicker, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use crate::runners::step_failure;
use crate::sink::VTubeSink;
use crate::vocabulary::{config_keys, picker_keys, sub_action_ids};

const HOTKEY_ID: PickedTextField = PickedTextField {
    key: config_keys::HOTKEY_ID,
    label: "Hotkey",
    picker: TextPicker::Options {
        options_key: picker_keys::HOTKEY_IDS,
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&HOTKEY_ID]);

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
        sub_action_ids::HOTKEY_TRIGGER
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let hotkey_id = match HOTKEY_ID.read(config, ctx.arg_stack) {
            Ok(hotkey_id) => hotkey_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = step_failure::outcome(&self.sink.trigger_hotkey(&hotkey_id).await);
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::VTubeError;
    use crate::runners::test_support::{MockSink, make_ctx};
    use forge_types::SubActionOutcome;
    use forge_types::Variant;
    use std::collections::BTreeMap;

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
        async fn deactivate_all_expressions(&self) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn move_model(
            &self,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: f64,
        ) -> Result<(), VTubeError> {
            Ok(())
        }
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
        async fn get_current_model(&self) -> Result<Variant, VTubeError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
        async fn get_hotkeys(&self) -> Result<Variant, VTubeError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
        async fn get_expressions(&self) -> Result<Variant, VTubeError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
        async fn get_parameters(&self) -> Result<Variant, VTubeError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
        async fn get_items(&self) -> Result<Variant, VTubeError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
        async fn pin_item(
            &self,
            _: &str,
            _: bool,
            _: &str,
            _: &str,
            _: &str,
            _: &str,
            _: &str,
            _: f64,
            _: f64,
        ) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn load_item(
            &self,
            _: &str,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<i64>,
            _: bool,
        ) -> Result<Variant, VTubeError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
        async fn unload_all_items(&self) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn tint_all_art_meshes(
            &self,
            _: i64,
            _: i64,
            _: i64,
            _: i64,
            _: Option<f64>,
        ) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn set_physics_override(&self, _: f64, _: f64) -> Result<(), VTubeError> {
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
