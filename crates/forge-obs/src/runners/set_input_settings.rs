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
        "obs.sources.set_input_settings"
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
                kind: "obs.sources.set_input_settings".to_owned(),
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};

    fn runner() -> SetInputSettingsRunner {
        SetInputSettingsRunner::new(Arc::new(MockSink))
    }

    #[test]
    fn validate_config_accepts_valid_json_object() {
        let config = BTreeMap::from([
            ("source".to_owned(), Variant::String("Webcam".to_owned())),
            (
                "settings_json".to_owned(),
                Variant::String(r#"{"width":1920}"#.to_owned()),
            ),
            ("overlay".to_owned(), Variant::Bool(true)),
        ]);
        assert!(runner().validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_malformed_json() {
        let config = BTreeMap::from([
            ("source".to_owned(), Variant::String("Webcam".to_owned())),
            (
                "settings_json".to_owned(),
                Variant::String("{not valid".to_owned()),
            ),
        ]);
        assert!(runner().validate_config(&config).is_err());
    }

    #[test]
    fn validate_config_rejects_non_string_settings() {
        let config = BTreeMap::from([
            ("source".to_owned(), Variant::String("Webcam".to_owned())),
            ("settings_json".to_owned(), Variant::Int(0)),
        ]);
        assert!(runner().validate_config(&config).is_err());
    }

    #[tokio::test]
    async fn execute_reports_success_with_correct_kind() {
        let stack = ArgStack::new();
        let config = BTreeMap::from([
            ("source".to_owned(), Variant::String("Webcam".to_owned())),
            (
                "settings_json".to_owned(),
                Variant::String(r#"{"width":1920}"#.to_owned()),
            ),
            ("overlay".to_owned(), Variant::Bool(true)),
        ]);
        let (tel, extra) = runner().execute(&config, &make_ctx(&stack)).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.sources.set_input_settings");
        assert!(extra.is_none());
    }
}
