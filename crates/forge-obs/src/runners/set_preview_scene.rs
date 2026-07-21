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

pub struct SetPreviewSceneRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetPreviewSceneRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetPreviewSceneRunner {
    fn id(&self) -> &str {
        "obs.scenes.set_preview"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Preview Scene"
    }

    fn summary(&self) -> &str {
        "Sets the current preview scene in OBS Studio Mode."
    }

    fn search_text(&self) -> &str {
        "obs scene preview studio mode set switch"
    }

    fn icon_name(&self) -> &str {
        "monitor"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("scene".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "scene",
            label: "Scene",
            options_key: "obs.scene_names",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        if matches!(config.get("scene"), Some(Variant::String(_))) {
            Ok(())
        } else {
            Err(RegistryError::InvalidConfig(
                "obs.scenes.set_preview: 'scene' must be a string".to_owned(),
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

        let raw_scene = config.str("scene").unwrap_or_default();

        let scene = ctx.arg_stack.interpolate(raw_scene);

        let outcome = SubActionOutcome::from_result(&self.sink.set_preview_scene(&scene).await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.scenes.set_preview".to_owned(),
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
    fn validate_config_accepts_scene_string() {
        let runner = SetPreviewSceneRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([("scene".to_owned(), Variant::String("Intro".to_owned()))]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_missing_scene() {
        let runner = SetPreviewSceneRunner::new(Arc::new(MockSink));
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }

    #[test]
    fn validate_config_rejects_non_string_scene() {
        let runner = SetPreviewSceneRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([("scene".to_owned(), Variant::Int(3))]);
        assert!(runner.validate_config(&config).is_err());
    }

    #[tokio::test]
    async fn execute_reports_success_with_correct_kind() {
        let runner = SetPreviewSceneRunner::new(Arc::new(MockSink));
        let stack = ArgStack::new();
        let config = BTreeMap::from([("scene".to_owned(), Variant::String("Intro".to_owned()))]);
        let (tel, extra) = runner.execute(&config, &make_ctx(&stack)).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.scenes.set_preview");
        assert!(extra.is_none());
    }
}
