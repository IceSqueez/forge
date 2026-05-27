use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner,
};
use forge_registry::runner::SubActionConfig;
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
            _ => Err(RegistryError::UnknownKindId(
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

        let raw = config
            .get("scene")
            .and_then(|v| if let Variant::String(s) = v { Some(s.as_str()) } else { None })
            .unwrap_or_default();
        let scene = ctx.arg_stack.interpolate(raw);

        let outcome = match self.sink.set_scene(&scene).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
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
    use crate::ObsError;
    use forge_types::Variant;

    struct MockSink;

    #[async_trait]
    impl ObsSink for MockSink {
        async fn set_scene(&self, _: &str) -> Result<(), ObsError> {
            Ok(())
        }
        async fn set_source_visible(&self, _: &str, _: &str, _: bool) -> Result<(), ObsError> {
            Ok(())
        }
        async fn set_input_mute(&self, _: &str, _: bool) -> Result<(), ObsError> {
            Ok(())
        }
        async fn start_record(&self) -> Result<(), ObsError> {
            Ok(())
        }
        async fn stop_record(&self) -> Result<(), ObsError> {
            Ok(())
        }
        async fn start_stream(&self) -> Result<(), ObsError> {
            Ok(())
        }
        async fn stop_stream(&self) -> Result<(), ObsError> {
            Ok(())
        }
        async fn raw_request(
            &self,
            _: &str,
            _: &Variant,
        ) -> Result<Variant, ObsError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
    }

    #[test]
    fn validate_config_accepts_scene_string() {
        let runner = SwitchCurrentSceneRunner::new(Arc::new(MockSink));
        let config =
            BTreeMap::from([("scene".to_owned(), Variant::String("Gameplay".to_owned()))]);
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
