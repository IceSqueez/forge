use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
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
        let source_ok = matches!(config.get("source"), Some(Variant::String(_)));
        if !source_ok {
            return Err(RegistryError::UnknownKindId(
                "obs.sources.set_input_settings: 'source' must be a string".to_owned(),
            ));
        }

        let json_ok = config.get("settings_json").is_some_and(|v| {
            if let Variant::String(s) = v {
                serde_json::from_str::<serde_json::Value>(s).is_ok()
            } else {
                false
            }
        });
        if !json_ok {
            return Err(RegistryError::UnknownKindId(
                "obs.sources.set_input_settings: 'settings_json' must be valid JSON".to_owned(),
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

        let raw_source = config
            .get("source")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let source = ctx.arg_stack.interpolate(raw_source);
        let overlay = matches!(config.get("overlay"), Some(Variant::Bool(true)));

        let settings_variant = config
            .get("settings_json")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    let json_val = serde_json::from_str::<serde_json::Value>(s).ok()?;
                    Variant::from_json(json_val).ok()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| Variant::Object(BTreeMap::new()));

        let outcome = match self
            .sink
            .set_input_settings(&source, &settings_variant, overlay)
            .await
        {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
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
    fn validate_config_rejects_missing_source() {
        let config =
            BTreeMap::from([("settings_json".to_owned(), Variant::String("{}".to_owned()))]);
        assert!(runner().validate_config(&config).is_err());
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
