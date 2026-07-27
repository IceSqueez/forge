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

use crate::sink::VTubeSink;

pub struct ItemMoveRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ItemMoveRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

const FADE_MODES: &[&str] = &[
    "linear",
    "easeIn",
    "easeOut",
    "easeBoth",
    "overshoot",
    "zip",
];

fn read_opt_float(config: &SubActionConfig, key: &str) -> Option<f64> {
    match config.get(key) {
        Some(Variant::Float(f)) => Some(*f),
        _ => None,
    }
}

fn read_opt_int(config: &SubActionConfig, key: &str) -> Option<i64> {
    match config.get(key) {
        Some(Variant::Int(i)) => Some(*i),
        _ => None,
    }
}

#[async_trait]
impl SubActionRunner for ItemMoveRunner {
    fn id(&self) -> &str {
        "vtube.item.move"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Move Item"
    }

    fn summary(&self) -> &str {
        "Moves, resizes, or rotates a loaded VTube Studio item on screen."
    }

    fn search_text(&self) -> &str {
        "vtube item move position size rotate order fade vts"
    }

    fn icon_name(&self) -> &str {
        "arrows-move"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (
                "item_instance_id".to_owned(),
                Variant::String(String::new()),
            ),
            ("fade_mode".to_owned(), Variant::String("linear".to_owned())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "item_instance_id",
                label: "Item Instance ID",
                placeholder: "item instance id from VTS",
            },
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
                key: "size",
                label: "Size",
                inner: Box::new(FormField::Text {
                    key: "size",
                    label: "Size",
                    placeholder: "0.0 to 1.0",
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
                key: "order",
                label: "Order",
                inner: Box::new(FormField::Integer {
                    key: "order",
                    label: "Order",
                    min: -1000,
                    max: 1000,
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
            FormField::Select {
                key: "fade_mode",
                label: "Fade Mode",
                options: FADE_MODES,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("item_instance_id") {
            Some(Variant::String(_)) => Ok(()),
            _ => Err(RegistryError::InvalidConfig(
                "vtube.item.move: 'item_instance_id' must be a string".to_owned(),
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

        let raw_id = config.str("item_instance_id").unwrap_or_default();
        let item_instance_id = ctx.arg_stack.interpolate(raw_id);

        let x = read_opt_float(config, "x");
        let y = read_opt_float(config, "y");
        let size = read_opt_float(config, "size");
        let rotation = read_opt_float(config, "rotation");
        let order = read_opt_int(config, "order");
        let duration = read_opt_float(config, "duration");
        let fade_mode = match config.get("fade_mode") {
            Some(Variant::String(s)) if FADE_MODES.contains(&s.as_str()) => s.as_str(),
            _ => "linear",
        };

        let outcome = if item_instance_id.is_empty() {
            SubActionOutcome::Failed("vtube.item.move: item_instance_id is empty".to_owned())
        } else if x.is_none()
            && y.is_none()
            && size.is_none()
            && rotation.is_none()
            && order.is_none()
        {
            SubActionOutcome::Success
        } else {
            let time_in_seconds = duration.unwrap_or(0.0);
            SubActionOutcome::from_result(
                &self
                    .sink
                    .move_item(
                        &item_instance_id,
                        x,
                        y,
                        size,
                        rotation,
                        order,
                        time_in_seconds,
                        fade_mode,
                    )
                    .await,
            )
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "vtube.item.move".to_owned(),
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

    fn config_with_id(id: &str) -> SubActionConfig {
        BTreeMap::from([(
            "item_instance_id".to_owned(),
            Variant::String(id.to_owned()),
        )])
    }

    #[tokio::test]
    async fn execute_valid_id_no_positional_fields_is_noop() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&config_with_id("item-1"), &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(extra.is_none());
        assert!(
            !sink.was_called(),
            "sink must not be called when no positional field is set"
        );
    }

    #[tokio::test]
    async fn execute_empty_id_is_failed_without_calling_sink() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&runner.default_config(), &ctx).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
        assert!(
            !sink.was_called(),
            "empty id must short-circuit before reaching the sink"
        );
    }

    #[tokio::test]
    async fn execute_dispatches_when_a_positional_field_is_set() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let mut config = config_with_id("item-1");
        config.insert("x".to_owned(), Variant::Float(0.5));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(sink.was_called());
    }

    #[tokio::test]
    async fn execute_failed_outcome_when_sink_errors() {
        let sink = Arc::new(MockSink::failing());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let mut config = config_with_id("item-1");
        config.insert("size".to_owned(), Variant::Float(0.3));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
    }

    #[test]
    fn validate_config_accepts_string_id() {
        let runner = ItemMoveRunner::new(Arc::new(MockSink::new()));
        assert!(runner.validate_config(&config_with_id("item-1")).is_ok());
    }

    #[test]
    fn validate_config_rejects_non_string_id() {
        let runner = ItemMoveRunner::new(Arc::new(MockSink::new()));
        let int_id = BTreeMap::from([("item_instance_id".to_owned(), Variant::Int(7))]);
        assert!(runner.validate_config(&int_id).is_err());
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }

    struct CaptureSink {
        last_id: Arc<std::sync::Mutex<Option<String>>>,
        last_fade_mode: Arc<std::sync::Mutex<Option<String>>>,
        last_x: Arc<std::sync::Mutex<Option<Option<f64>>>>,
    }

    impl CaptureSink {
        fn new() -> Self {
            Self {
                last_id: Arc::new(std::sync::Mutex::new(None)),
                last_fade_mode: Arc::new(std::sync::Mutex::new(None)),
                last_x: Arc::new(std::sync::Mutex::new(None)),
            }
        }

        fn id(&self) -> Option<String> {
            self.last_id.lock().unwrap().clone()
        }

        fn fade_mode(&self) -> Option<String> {
            self.last_fade_mode.lock().unwrap().clone()
        }

        fn x(&self) -> Option<Option<f64>> {
            *self.last_x.lock().unwrap()
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
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: f64,
        ) -> Result<(), VTubeError> {
            Ok(())
        }
        #[allow(clippy::too_many_arguments)]
        async fn move_item(
            &self,
            item_instance_id: &str,
            x: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<i64>,
            _: f64,
            fade_mode: &str,
        ) -> Result<(), VTubeError> {
            *self.last_id.lock().unwrap() = Some(item_instance_id.to_owned());
            *self.last_x.lock().unwrap() = Some(x);
            *self.last_fade_mode.lock().unwrap() = Some(fade_mode.to_owned());
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
        #[allow(clippy::too_many_arguments)]
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
        #[allow(clippy::too_many_arguments)]
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
    async fn execute_forwards_resolved_id_and_positional_arg_to_sink() {
        let sink = Arc::new(CaptureSink::new());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack =
            ArgStack::new().set("item".to_owned(), Variant::String("resolved-42".to_owned()));
        let mut config = config_with_id("%item%");
        config.insert("x".to_owned(), Variant::Float(0.5));
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(
            sink.id().expect("sink must have been called"),
            "resolved-42",
            "item_instance_id must be interpolated before reaching the sink"
        );
        let x = sink
            .x()
            .expect("sink must have been called")
            .expect("x was set");
        assert!((x - 0.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn execute_invalid_fade_mode_falls_back_to_linear() {
        let sink = Arc::new(CaptureSink::new());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let mut config = config_with_id("item-1");
        config.insert("rotation".to_owned(), Variant::Float(10.0));
        config.insert("fade_mode".to_owned(), Variant::String("wobble".to_owned()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        runner.execute(&config, &ctx).await;
        assert_eq!(
            sink.fade_mode().expect("sink must have been called"),
            "linear",
            "an unrecognised fade_mode must fall back to linear"
        );
    }

    #[tokio::test]
    async fn execute_valid_fade_mode_is_forwarded_unchanged() {
        let sink = Arc::new(CaptureSink::new());
        let runner = ItemMoveRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let mut config = config_with_id("item-1");
        config.insert("rotation".to_owned(), Variant::Float(10.0));
        config.insert("fade_mode".to_owned(), Variant::String("easeIn".to_owned()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        runner.execute(&config, &ctx).await;
        assert_eq!(
            sink.fade_mode().expect("sink must have been called"),
            "easeIn",
            "a recognised fade_mode must pass through verbatim"
        );
    }
}
