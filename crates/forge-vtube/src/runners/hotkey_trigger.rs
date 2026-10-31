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
    async fn execute_passes_interpolated_id_to_sink() {
        let sink = Arc::new(MockSink::new());
        let runner = HotkeyTriggerRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new().set("id".to_owned(), Variant::String("hk-abc".to_owned()));
        let config = BTreeMap::from([("hotkey_id".to_owned(), Variant::String("%id%".to_owned()))]);
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(extra.is_none());
        assert_eq!(
            sink.strings_sent_to("trigger_hotkey"),
            Some(vec!["hk-abc".to_owned()]),
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
