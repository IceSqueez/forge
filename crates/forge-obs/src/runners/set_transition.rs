use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct SetTransitionRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetTransitionRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetTransitionRunner {
    fn id(&self) -> &str {
        "obs.scenes.set_transition"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Scene Transition"
    }

    fn summary(&self) -> &str {
        "Sets the active scene transition by name."
    }

    fn search_text(&self) -> &str {
        "obs transition fade cut swipe set scene change"
    }

    fn icon_name(&self) -> &str {
        "shuffle"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("transition".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "transition",
            label: "Transition Name",
            placeholder: "e.g. Fade",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        if matches!(config.get("transition"), Some(Variant::String(_))) {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "obs.scenes.set_transition: 'transition' must be a string".to_owned(),
            ))
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_transition = config
            .get("transition")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let transition = ctx.arg_stack.interpolate(raw_transition);

        let outcome = match self.sink.set_current_scene_transition(&transition).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "obs.scenes.set_transition".to_owned(),
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

    #[test]
    fn validate_config_accepts_transition_string() {
        let runner = SetTransitionRunner::new(Arc::new(MockSink));
        let config =
            BTreeMap::from([("transition".to_owned(), Variant::String("Fade".to_owned()))]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_missing_transition() {
        let runner = SetTransitionRunner::new(Arc::new(MockSink));
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }

    #[test]
    fn validate_config_rejects_non_string_transition() {
        let runner = SetTransitionRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([("transition".to_owned(), Variant::Bool(true))]);
        assert!(runner.validate_config(&config).is_err());
    }

    #[tokio::test]
    async fn execute_reports_success_with_correct_kind() {
        let runner = SetTransitionRunner::new(Arc::new(MockSink));
        let stack = ArgStack::new();
        let config = BTreeMap::from([("transition".to_owned(), Variant::String("Cut".to_owned()))]);
        let (tel, extra) = runner.execute(&config, &make_ctx(&stack)).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.scenes.set_transition");
        assert!(extra.is_none());
    }
}
