use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{RunContext, SubActionCategory, SubActionIo, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::{sub_action_ids, variables};

const KIND_ID: &str = sub_action_ids::SCENE_GET_LIST;

pub(crate) struct QuerySceneListRunner {
    sink: Arc<dyn ObsSink>,
}

impl QuerySceneListRunner {
    pub(crate) fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for QuerySceneListRunner {
    fn id(&self) -> &str {
        KIND_ID
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

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            fixed_outputs: vec![
                variables::scene_query::ALL_NAMES.to_owned(),
                variables::scene_query::CURRENT.to_owned(),
            ],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        match self.sink.get_scene_list().await {
            Ok(scenes) => {
                let all_names =
                    Variant::Array(scenes.names.into_iter().map(Variant::String).collect());
                let stack = ctx
                    .arg_stack
                    .clone()
                    .set(variables::scene_query::ALL_NAMES.to_owned(), all_names)
                    .set(
                        variables::scene_query::CURRENT.to_owned(),
                        Variant::String(scenes.current),
                    );
                (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
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
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
                    outcome: step_failure::failed(&e),
                    index: ctx.index,
                },
                None,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use std::collections::BTreeMap;

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
