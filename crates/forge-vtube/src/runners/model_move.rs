use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct ModelMoveRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ModelMoveRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

fn read_opt_float(config: &SubActionConfig, key: &str) -> Option<f64> {
    match config.get(key) {
        Some(Variant::Float(f)) => Some(*f),
        _ => None,
    }
}

#[async_trait]
impl SubActionRunner for ModelMoveRunner {
    fn id(&self) -> &str {
        "vtube.model.move"
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Optional {
                key: "x",
                label: "X Position",
                inner: Box::new(FormField::Text {
                    key: "x",
                    label: "X Position",
                    placeholder: "-1.0 to 1.0",
                }),
            },
            FormField::Optional {
                key: "y",
                label: "Y Position",
                inner: Box::new(FormField::Text {
                    key: "y",
                    label: "Y Position",
                    placeholder: "-1.0 to 1.0",
                }),
            },
            FormField::Optional {
                key: "rotation",
                label: "Rotation",
                inner: Box::new(FormField::Text {
                    key: "rotation",
                    label: "Rotation",
                    placeholder: "-360 to 360",
                }),
            },
            FormField::Optional {
                key: "duration",
                label: "Duration (s)",
                inner: Box::new(FormField::Text {
                    key: "duration",
                    label: "Duration (s)",
                    placeholder: "0.5",
                }),
            },
        ]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let x = read_opt_float(config, "x");
        let y = read_opt_float(config, "y");
        let rotation = read_opt_float(config, "rotation");
        let duration = read_opt_float(config, "duration");

        let outcome = if x.is_none() && y.is_none() && rotation.is_none() {
            SubActionOutcome::Success
        } else {
            let time_in_seconds = duration.unwrap_or(0.0);
            match self.sink.move_model(x, y, rotation, time_in_seconds).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            }
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "vtube.model.move".to_owned(),
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::error::VTubeError;
    use crate::runners::test_support::{MockSink, make_ctx};

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
    async fn execute_partial_fields_dispatches() {
        let sink = Arc::new(MockSink::new());
        let runner = ModelMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let config = BTreeMap::from([
            ("x".to_owned(), Variant::Float(0.5)),
            ("duration".to_owned(), Variant::Float(0.3)),
        ]);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(sink.was_called());
    }

    #[tokio::test]
    async fn execute_full_move_dispatches_correctly() {
        let sink = Arc::new(MockSink::new());
        let runner = ModelMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let config = BTreeMap::from([
            ("x".to_owned(), Variant::Float(-0.2)),
            ("y".to_owned(), Variant::Float(0.1)),
            ("rotation".to_owned(), Variant::Float(45.0)),
            ("duration".to_owned(), Variant::Float(1.0)),
        ]);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "vtube.model.move");
        assert!(extra.is_none());
        assert!(sink.was_called());
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
            time_in_seconds: f64,
        ) -> Result<(), VTubeError> {
            *self.captured_x.lock().unwrap() = Some(x);
            *self.captured_y.lock().unwrap() = Some(y);
            *self.captured_rotation.lock().unwrap() = Some(rotation);
            *self.captured_duration.lock().unwrap() = Some(time_in_seconds);
            Ok(())
        }
        #[allow(clippy::too_many_arguments)]
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
