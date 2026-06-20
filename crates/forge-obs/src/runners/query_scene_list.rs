use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct QuerySceneListRunner {
    sink: Arc<dyn ObsSink>,
}

impl QuerySceneListRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for QuerySceneListRunner {
    fn id(&self) -> &str {
        "obs.scenes.get_list"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Get Scene List"
    }

    fn summary(&self) -> &str {
        "Queries OBS for all scenes and the current program scene."
    }

    fn search_text(&self) -> &str {
        "obs scenes get list all current program"
    }

    fn icon_name(&self) -> &str {
        "layout-grid"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        match self.sink.get_scene_list().await {
            Ok(variant) => {
                let mut stack = ArgStack::new();
                if let Variant::Object(ref map) = variant {
                    if let Some(names) = map.get("all_names") {
                        stack = stack.set("obs.scenes.all_names".to_owned(), names.clone());
                    }
                    if let Some(current) = map.get("current") {
                        stack = stack.set("obs.scenes.current".to_owned(), current.clone());
                    }
                }
                (
                    SubActionTelemetry {
                        kind: "obs.scenes.get_list".to_owned(),
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
                    kind: "obs.scenes.get_list".to_owned(),
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

    #[tokio::test]
    async fn execute_populates_scene_names_and_current_from_sink() {
        let runner = QuerySceneListRunner::new(Arc::new(MockSink));
        let empty = ArgStack::new();
        let (telemetry, stack) = runner.execute(&BTreeMap::new(), &make_ctx(&empty)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        let stack = stack.unwrap();
        assert_eq!(
            stack.get("obs.scenes.all_names"),
            Some(&Variant::Array(vec![
                Variant::String("Intro".to_owned()),
                Variant::String("Gameplay".to_owned()),
            ])),
        );
        assert_eq!(
            stack.get("obs.scenes.current"),
            Some(&Variant::String("Gameplay".to_owned())),
        );
    }
}
