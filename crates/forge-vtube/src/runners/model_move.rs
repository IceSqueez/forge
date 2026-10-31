use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, NumberField, OptionalField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use crate::runners::fields::{DURATION, IMMEDIATE_SECONDS, ROTATION, X, Y, optional_number};
use crate::runners::step_failure;
use crate::sink::VTubeSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const SIZE: OptionalField<NumberField> = optional_number(config_keys::SIZE, "Size", "-100 to 100");

const CONFIG: ConfigSpec = ConfigSpec::new(&[&X, &Y, &ROTATION, &SIZE, &DURATION]);

struct MoveTarget {
    x: Option<f64>,
    y: Option<f64>,
    rotation: Option<f64>,
    size: Option<f64>,
    duration: Option<f64>,
}

impl MoveTarget {
    fn is_empty(&self) -> bool {
        self.x.is_none() && self.y.is_none() && self.rotation.is_none() && self.size.is_none()
    }
}

fn read_target(config: &SubActionConfig, args: &ArgStack) -> Result<MoveTarget, ConfigProblem> {
    Ok(MoveTarget {
        x: X.read(config, args)?,
        y: Y.read(config, args)?,
        rotation: ROTATION.read(config, args)?,
        size: SIZE.read(config, args)?,
        duration: DURATION.read(config, args)?,
    })
}

pub struct ModelMoveRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ModelMoveRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ModelMoveRunner {
    fn id(&self) -> &str {
        sub_action_ids::MODEL_MOVE
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Move Model"
    }

    fn summary(&self) -> &str {
        "Moves or rotates the VTube Studio model on screen."
    }

    fn search_text(&self) -> &str {
        "vtube model move position rotate x y vts"
    }

    fn icon_name(&self) -> &str {
        "arrows-move"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let target = match read_target(config, ctx.arg_stack) {
            Ok(target) => target,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        if target.is_empty() {
            return (timer.success(), None);
        }
        let outcome = step_failure::outcome(
            &self
                .sink
                .move_model(
                    target.x,
                    target.y,
                    target.rotation,
                    target.size,
                    target.duration.unwrap_or(IMMEDIATE_SECONDS),
                )
                .await,
        );
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::runners::test_support::{MockSink, make_ctx};
    use forge_types::SubActionOutcome;
    use forge_types::Variant;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn execute_all_none_is_noop() {
        let sink = Arc::new(MockSink::new());
        let runner = ModelMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&BTreeMap::new(), &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(extra.is_none());
        assert!(
            !sink.was_called(),
            "sink must not be called when all fields omitted"
        );
    }

    #[tokio::test]
    async fn execute_duration_only_without_coords_is_noop() {
        let sink = Arc::new(MockSink::new());
        let runner = ModelMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let config = BTreeMap::from([("duration".to_owned(), Variant::Float(0.5))]);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(extra.is_none());
        assert!(
            !sink.was_called(),
            "sink must not be called when only duration is set and x/y/rotation are absent"
        );
    }

    #[tokio::test]
    async fn execute_passes_correct_args_to_sink() {
        let sink = Arc::new(MockSink::new());
        let runner = ModelMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let config = BTreeMap::from([
            ("x".to_owned(), Variant::Float(0.3)),
            ("rotation".to_owned(), Variant::Float(90.0)),
            ("duration".to_owned(), Variant::Float(0.25)),
        ]);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        let sent = sink
            .numbers_sent_to("move_model")
            .expect("sink must have been called with coordinate arguments");
        let [x, y, rot, _size, dur] = sent.as_slice() else {
            panic!("unexpected argument count: {sent:?}");
        };
        assert!(x.is_some(), "x should have been passed to sink");
        assert!((x.unwrap() - 0.3).abs() < f64::EPSILON);
        assert!(y.is_none(), "y was not specified in config");
        assert!((rot.unwrap() - 90.0).abs() < f64::EPSILON);
        assert!((dur.unwrap() - 0.25).abs() < f64::EPSILON);
    }
}
