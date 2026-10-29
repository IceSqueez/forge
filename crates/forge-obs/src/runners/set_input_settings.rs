use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextEditor, TextField, ToggleField, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use serde_json::{Map, Value};

use crate::ObsSink;
use crate::runners::json_object_field::JsonObjectField;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::SOURCE_SET_INPUT_SETTINGS;

const SOURCE: TextField = TextField {
    key: config_keys::SOURCE,
    label: "Input Source",
    editor: TextEditor::Line {
        placeholder: "e.g. Webcam",
    },
    default: "",
    required: true,
};

const SETTINGS_JSON: JsonObjectField = JsonObjectField {
    key: config_keys::SETTINGS_JSON,
    label: "Settings (JSON)",
    default: "{}",
};

const OVERLAY: ToggleField = ToggleField {
    key: config_keys::OVERLAY,
    label: "Overlay (merge with existing settings)",
    default: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &SETTINGS_JSON, &OVERLAY]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, Map<String, Value>, bool), ConfigProblem> {
    Ok((
        SOURCE.read(config, args)?,
        SETTINGS_JSON.read(config, args)?,
        OVERLAY.read(config, args)?,
    ))
}

pub struct SetInputSettingsRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetInputSettingsRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetInputSettingsRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Input Settings"
    }

    fn summary(&self) -> &str {
        "Applies a JSON settings object to an OBS input source."
    }

    fn search_text(&self) -> &str {
        "obs input source settings json apply configure properties"
    }

    fn icon_name(&self) -> &str {
        "settings"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let (source, settings, overlay) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = SubActionOutcome::from_result(
            &self
                .sink
                .set_input_settings(&source, &settings, overlay)
                .await,
        );
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, RecordingSink, make_ctx};
    use forge_registry::RegistryError;
    use forge_types::Variant;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn config_with(settings: Variant, overlay: bool) -> SubActionConfig {
        BTreeMap::from([
            ("source".to_owned(), Variant::String("Webcam".to_owned())),
            ("settings_json".to_owned(), settings),
            ("overlay".to_owned(), Variant::Bool(overlay)),
        ])
    }

    #[test]
    fn validate_config_accepts_valid_json_object() {
        let runner = SetInputSettingsRunner::new(Arc::new(MockSink));
        let config = config_with(Variant::String(r#"{"width":1920}"#.to_owned()), true);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[tokio::test]
    async fn execute_hands_the_sink_the_user_object_unchanged_for_each_overlay_flag() {
        let settings = json!({
            "url": null,
            "transform": {"crop": {"left": 4, "label": null}, "flip": true},
            "source": {"type": "x", "value": 1},
            "items": [1, null, "two"],
        });
        let text = serde_json::to_string(&settings).unwrap();
        let serde_json::Value::Object(expected) = settings else {
            unreachable!()
        };
        for overlay in [true, false] {
            let sink = RecordingSink::new();
            let runner = SetInputSettingsRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
            let stack = ArgStack::new();
            let config = config_with(Variant::String(text.clone()), overlay);

            let (tel, _) = runner.execute(&config, &make_ctx(&stack)).await;

            assert_eq!(tel.outcome, SubActionOutcome::Success, "overlay={overlay}");
            assert_eq!(
                sink.input_settings_calls(),
                vec![("Webcam".to_owned(), expected.clone(), overlay)],
                "overlay={overlay}",
            );
        }
    }

    #[tokio::test]
    async fn settings_that_are_not_a_json_object_fail_validation_and_execution_without_a_sink_call()
    {
        let cases = [
            ("malformed", Variant::String("{not valid".to_owned())),
            ("array", Variant::String("[1,2]".to_owned())),
            ("number", Variant::String("5".to_owned())),
            ("string", Variant::String(r#""str""#.to_owned())),
            ("null", Variant::String("null".to_owned())),
            ("non-string config value", Variant::Int(0)),
        ];
        for (label, settings) in cases {
            let sink = RecordingSink::new();
            let runner = SetInputSettingsRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
            let config = config_with(settings, true);

            let validation = runner.validate_config(&config);
            assert!(
                matches!(
                    validation,
                    Err(RegistryError::InvalidConfig(ref problem))
                        if problem.field.as_deref() == Some("settings_json")
                ),
                "{label}: validate_config must reject settings_json, got {validation:?}",
            );

            let stack = ArgStack::new();
            let (tel, _) = runner.execute(&config, &make_ctx(&stack)).await;
            assert!(
                matches!(&tel.outcome, SubActionOutcome::Failed(reason) if reason.starts_with("invalid step config: 'settings_json' ")),
                "{label}: execute must fail with a settings_json reason, got {:?}",
                tel.outcome,
            );
            assert!(sink.calls().is_empty(), "{label}: sink must not be called");
        }
    }
}
