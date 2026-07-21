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

pub struct SwitchCurrentSceneRunner {
    sink: Arc<dyn ObsSink>,
}

impl SwitchCurrentSceneRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SwitchCurrentSceneRunner {
    fn id(&self) -> &str {
        "obs.scenes.switch_current"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Switch Scene"
    }

    fn summary(&self) -> &str {
        "Sets the current OBS program scene."
    }

    fn search_text(&self) -> &str {
        "obs switch scene current program set"
    }

    fn icon_name(&self) -> &str {
        "arrows-shuffle"
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
        match config.get("scene") {
            Some(Variant::String(_)) => Ok(()),
            _ => Err(RegistryError::InvalidConfig(
                "obs.scenes.switch_current: 'scene' must be a string".to_owned(),
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

        let raw = config.str("scene").unwrap_or_default();
        let scene = ctx.arg_stack.interpolate(raw);

        let outcome = SubActionOutcome::from_result(&self.sink.set_scene(&scene).await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.scenes.switch_current".to_owned(),
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
    use crate::runners::test_support::MockSink;

    #[test]
    fn validate_config_accepts_scene_string() {
        let runner = SwitchCurrentSceneRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([("scene".to_owned(), Variant::String("Gameplay".to_owned()))]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_missing_scene() {
        let runner = SwitchCurrentSceneRunner::new(Arc::new(MockSink));
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }

    #[test]
    fn validate_config_rejects_non_string_scene() {
        let runner = SwitchCurrentSceneRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([("scene".to_owned(), Variant::Int(1))]);
        assert!(runner.validate_config(&config).is_err());
    }
}
