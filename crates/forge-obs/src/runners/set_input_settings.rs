use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use serde_json::{Map, Value};
use time::OffsetDateTime;

use crate::ObsSink;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::SOURCE_SET_INPUT_SETTINGS;

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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("source".to_owned(), Variant::String(String::new())),
            ("settings_json".to_owned(), Variant::String("{}".to_owned())),
            ("overlay".to_owned(), Variant::Bool(true)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "source",
                label: "Input Source",
                placeholder: "e.g. Webcam",
            },
            FormField::TextArea {
                key: "settings_json",
                label: "Settings (JSON)",
            },
            FormField::Toggle {
                key: "overlay",
                label: "Overlay (merge with existing settings)",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let source_ok =
            matches!(config.get("source"), Some(Variant::String(s)) if !s.trim().is_empty());
        if !source_ok {
            return Err(RegistryError::invalid_field(
                "source",
                ConfigReason::Required,
            ));
        }

        if let Err(reason) = parse_settings(config) {
            return Err(RegistryError::invalid_field(
                "settings_json",
                ConfigReason::InvalidFormat(reason),
            ));
        }

        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_source = config.str("source").unwrap_or_default();

        let source = ctx.arg_stack.interpolate(raw_source);
        let overlay = matches!(config.get("overlay"), Some(Variant::Bool(true)));

        let outcome = match parse_settings(config) {
            Ok(settings) => SubActionOutcome::from_result(
                &self
                    .sink
                    .set_input_settings(&source, &settings, overlay)
                    .await,
            ),
            Err(reason) => SubActionOutcome::Failed(format!("settings_json: {reason}")),
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

fn parse_settings(config: &SubActionConfig) -> Result<Map<String, Value>, String> {
    let Some(Variant::String(text)) = config.get("settings_json") else {
        return Err("expected JSON object text".to_owned());
    };
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(settings)) => Ok(settings),
        Ok(_) => Err("expected a JSON object".to_owned()),
        Err(e) => Err(format!("not valid JSON ({e})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, RecordingSink, make_ctx};
    use forge_registry::RegistryError;
    use serde_json::json;

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
                matches!(&tel.outcome, SubActionOutcome::Failed(reason) if reason.starts_with("settings_json: ")),
                "{label}: execute must fail with a settings_json reason, got {:?}",
                tel.outcome,
            );
            assert!(sink.calls().is_empty(), "{label}: sink must not be called");
        }
    }
}
