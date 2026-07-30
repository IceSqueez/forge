use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct QueryInputSettingsRunner {
    sink: Arc<dyn ObsSink>,
}

impl QueryInputSettingsRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for QueryInputSettingsRunner {
    fn id(&self) -> &str {
        "obs.sources.get_input_settings"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Get Input Settings"
    }

    fn summary(&self) -> &str {
        "Queries OBS for the current settings object of a named input source."
    }

    fn search_text(&self) -> &str {
        "obs source input get settings properties kind"
    }

    fn icon_name(&self) -> &str {
        "settings"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("source".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "source",
            label: "Input name",
            placeholder: "e.g. Webcam",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("source") {
            Some(Variant::String(s)) if !s.trim().is_empty() => Ok(()),
            Some(Variant::String(_)) => Err(RegistryError::InvalidConfig(
                "obs.sources.get_input_settings: 'source' must not be empty".to_owned(),
            )),
            _ => Err(RegistryError::InvalidConfig(
                "obs.sources.get_input_settings: 'source' must be a string".to_owned(),
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

        let raw_source = config.str("source").unwrap_or_default();
        let source = ctx.arg_stack.interpolate(raw_source);

        match self.sink.get_input_settings(&source).await {
            Ok(variant) => {
                let mut stack = ArgStack::new();
                if let Variant::Object(ref map) = variant {
                    if let Some(v) = map.get("settings") {
                        stack = stack.set("obs.input.settings".to_owned(), v.clone());
                    }
                    if let Some(v) = map.get("kind") {
                        stack = stack.set("obs.input.kind".to_owned(), v.clone());
                    }
                }
                (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: "obs.sources.get_input_settings".to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Success,
                        index: ctx.index,
                    },
                    Some(stack),
                )
            }
            Err(e) => (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: "obs.sources.get_input_settings".to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(e.to_string()),
                    index: ctx.index,
                },
                None,
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};

    fn runner() -> QueryInputSettingsRunner {
        QueryInputSettingsRunner::new(Arc::new(MockSink))
    }

    #[tokio::test]
    async fn execute_populates_kind_and_settings_object_from_sink() {
        let config = BTreeMap::from([("source".to_owned(), Variant::String("Caption".to_owned()))]);
        let empty = ArgStack::new();
        let (telemetry, stack) = runner().execute(&config, &make_ctx(&empty)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        let stack = stack.unwrap();
        assert_eq!(
            stack.get("obs.input.kind"),
            Some(&Variant::String("text_ft2_source_v2".to_owned())),
        );
        let expected_settings = Variant::Object(BTreeMap::from([(
            "text".to_owned(),
            Variant::String("hello".to_owned()),
        )]));
        assert_eq!(stack.get("obs.input.settings"), Some(&expected_settings));
    }

    #[test]
    fn validate_config_takes_a_named_source_and_nothing_else() {
        let named = BTreeMap::from([("source".to_owned(), Variant::String("Webcam".to_owned()))]);
        assert!(runner().validate_config(&named).is_ok());

        for value in [
            None,
            Some(Variant::String(String::new())),
            Some(Variant::Bool(true)),
            Some(Variant::Int(3)),
        ] {
            let mut config = BTreeMap::new();
            if let Some(value) = value.clone() {
                config.insert("source".to_owned(), value);
            }
            assert!(
                runner().validate_config(&config).is_err(),
                "accepted source = {value:?}",
            );
        }
    }
}
