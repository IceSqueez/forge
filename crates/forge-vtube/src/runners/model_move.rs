use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, NumberField, OptionalField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::runners::fields::{DURATION, IMMEDIATE_SECONDS, ROTATION, X, Y, optional_number};
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
        let outcome = SubActionOutcome::from_result(
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
#[expect(clippy::type_complexity, reason = "test tuple of captured channels")]
mod tests {
    use super::*;
    use crate::error::VTubeError;
    use crate::runners::test_support::{MockSink, make_ctx};
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

    struct CaptureSink {
        captured_x: Arc<std::sync::Mutex<Option<Option<f64>>>>,
        captured_y: Arc<std::sync::Mutex<Option<Option<f64>>>>,
        captured_rotation: Arc<std::sync::Mutex<Option<Option<f64>>>>,
        captured_duration: Arc<std::sync::Mutex<Option<f64>>>,
    }

    impl CaptureSink {
        fn new() -> Self {
            Self {
                captured_x: Arc::new(std::sync::Mutex::new(None)),
                captured_y: Arc::new(std::sync::Mutex::new(None)),
                captured_rotation: Arc::new(std::sync::Mutex::new(None)),
                captured_duration: Arc::new(std::sync::Mutex::new(None)),
            }
        }

        fn get_call(&self) -> Option<(Option<f64>, Option<f64>, Option<f64>, f64)> {
            let x = *self.captured_x.lock().unwrap();
            let dur = *self.captured_duration.lock().unwrap();
            x.map(|xv| {
                let y = *self.captured_y.lock().unwrap();
                let r = *self.captured_rotation.lock().unwrap();
                (xv, y.flatten(), r.flatten(), dur.unwrap_or(0.0))
            })
        }
    }

    #[async_trait]
    impl VTubeSink for CaptureSink {
        async fn trigger_hotkey(&self, _: &str) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn set_expression(&self, _: &str, _: bool) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn set_param(&self, _: &str, _: f64) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn load_model(&self, _: &str) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn reset_params(&self) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn move_model(
            &self,
            x: Option<f64>,
            y: Option<f64>,
            rotation: Option<f64>,
            _size: Option<f64>,
            time_in_seconds: f64,
        ) -> Result<(), VTubeError> {
            *self.captured_x.lock().unwrap() = Some(x);
            *self.captured_y.lock().unwrap() = Some(y);
            *self.captured_rotation.lock().unwrap() = Some(rotation);
            *self.captured_duration.lock().unwrap() = Some(time_in_seconds);
            Ok(())
        }
        async fn move_item(
            &self,
            _: &str,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<i64>,
            _: f64,
            _: &str,
        ) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn get_current_model(&self) -> Result<Variant, VTubeError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
        async fn get_hotkeys(&self) -> Result<Variant, VTubeError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
        async fn get_expressions(&self) -> Result<Variant, VTubeError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
        async fn get_parameters(&self) -> Result<Variant, VTubeError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
        async fn get_items(&self) -> Result<Variant, VTubeError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
        async fn pin_item(
            &self,
            _: &str,
            _: bool,
            _: &str,
            _: &str,
            _: &str,
            _: &str,
            _: &str,
            _: f64,
            _: f64,
        ) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn load_item(
            &self,
            _: &str,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<i64>,
            _: bool,
        ) -> Result<Variant, VTubeError> {
            Ok(Variant::Object(BTreeMap::new()))
        }
        async fn unload_all_items(&self) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn tint_all_art_meshes(
            &self,
            _: i64,
            _: i64,
            _: i64,
            _: i64,
            _: Option<f64>,
        ) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn set_physics_override(&self, _: f64, _: f64) -> Result<(), VTubeError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn execute_passes_correct_args_to_sink() {
        let sink = Arc::new(CaptureSink::new());
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
        let (x, y, rot, dur) = sink
            .get_call()
            .expect("sink must have been called with coordinate arguments");
        assert!(x.is_some(), "x should have been passed to sink");
        assert!((x.unwrap() - 0.3).abs() < f64::EPSILON);
        assert!(y.is_none(), "y was not specified in config");
        assert!((rot.unwrap() - 90.0).abs() < f64::EPSILON);
        assert!((dur - 0.25).abs() < f64::EPSILON);
    }
}
