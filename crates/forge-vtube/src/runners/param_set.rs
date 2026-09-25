use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::runners::hold_registry::HoldRegistry;
use crate::runners::numeric::{accepts_number, optional_number, required_number};
use crate::sink::VTubeSink;

const PARAM_HOLD_RESEND_INTERVAL: Duration = Duration::from_millis(400);
const PARAM_HOLD_MAX_SECS: f64 = 600.0;

pub struct ParamSetRunner {
    sink: Arc<dyn VTubeSink>,
    holds: Arc<HoldRegistry>,
}

impl ParamSetRunner {
    pub(crate) fn new(sink: Arc<dyn VTubeSink>, holds: Arc<HoldRegistry>) -> Self {
        Self { sink, holds }
    }

    fn start_hold(&self, param_id: String, value: f64, hold_secs: f64) {
        let sink = Arc::clone(&self.sink);
        let resend_id = param_id.clone();
        let ticks = (hold_secs / PARAM_HOLD_RESEND_INTERVAL.as_secs_f64()).ceil() as u32;
        let handle = tokio::spawn(async move {
            for _ in 0..ticks {
                tokio::time::sleep(PARAM_HOLD_RESEND_INTERVAL).await;
                if sink.set_param(&resend_id, value).await.is_err() {
                    break;
                }
            }
        });
        self.holds.insert(param_id, handle);
    }
}

#[async_trait]
impl SubActionRunner for ParamSetRunner {
    fn id(&self) -> &str {
        "vtube.param.set"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Set Parameter"
    }

    fn summary(&self) -> &str {
        "Injects a value into a VTube Studio parameter, optionally held for a duration."
    }

    fn search_text(&self) -> &str {
        "vtube parameter inject set value hold face tracking vts"
    }

    fn icon_name(&self) -> &str {
        "sliders"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("param_id".to_owned(), Variant::String(String::new())),
            ("value".to_owned(), Variant::Float(0.0)),
            ("hold_secs".to_owned(), Variant::Float(0.0)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "param_id",
                label: "Parameter ID",
                placeholder: "MyCustomParam",
            },
            FormField::Text {
                key: "value",
                label: "Value",
                placeholder: "0.0",
            },
            FormField::Text {
                key: "hold_secs",
                label: "Hold (seconds, 0 = one-shot)",
                placeholder: "0",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("param_id") {
            Some(Variant::String(_)) => {}
            _ => {
                return Err(RegistryError::InvalidConfig(
                    "vtube.param.set: 'param_id' must be a string".to_owned(),
                ));
            }
        }
        if !accepts_number(config.get("value")) {
            return Err(RegistryError::InvalidConfig(
                "vtube.param.set: 'value' must be a number".to_owned(),
            ));
        }
        let hold_is_blank =
            matches!(config.get("hold_secs"), Some(Variant::String(s)) if s.trim().is_empty());
        if config.get("hold_secs").is_some()
            && !hold_is_blank
            && !accepts_number(config.get("hold_secs"))
        {
            return Err(RegistryError::InvalidConfig(
                "vtube.param.set: 'hold_secs' must be a number".to_owned(),
            ));
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_id = config.str("param_id").unwrap_or_default();
        let param_id = ctx.arg_stack.interpolate(raw_id);

        let outcome = match required_number(config, "value", ctx) {
            Ok(value) => match optional_number(config, "hold_secs", ctx) {
                Ok(hold_secs) => {
                    self.holds.abort(&param_id);
                    let result = self.sink.set_param(&param_id, value).await;
                    let hold_secs = hold_secs.unwrap_or(0.0).clamp(0.0, PARAM_HOLD_MAX_SECS);
                    if result.is_ok() && hold_secs > 0.0 {
                        self.start_hold(param_id.clone(), value, hold_secs);
                    }
                    SubActionOutcome::from_result(&result)
                }
                Err(reason) => SubActionOutcome::Failed(format!("vtube.param.set: {reason}")),
            },
            Err(reason) => SubActionOutcome::Failed(format!("vtube.param.set: {reason}")),
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "vtube.param.set".to_owned(),
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::error::VTubeError;
    use crate::runners::test_support::{MockSink, make_ctx};

    #[test]
    fn validate_config_accepts_valid_param() {
        let runner = ParamSetRunner::new(Arc::new(MockSink::new()));
        let config = BTreeMap::from([
            ("param_id".to_owned(), Variant::String("MyParam".to_owned())),
            ("value".to_owned(), Variant::Float(0.5)),
        ]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_missing_param_id() {
        let runner = ParamSetRunner::new(Arc::new(MockSink::new()));
        let config = BTreeMap::from([("value".to_owned(), Variant::Float(1.0))]);
        assert!(runner.validate_config(&config).is_err());
    }

    struct CaptureSink {
        last_id: Arc<std::sync::Mutex<Option<String>>>,
        last_value: Arc<std::sync::Mutex<Option<f64>>>,
    }

    impl CaptureSink {
        fn new() -> Self {
            Self {
                last_id: Arc::new(std::sync::Mutex::new(None)),
                last_value: Arc::new(std::sync::Mutex::new(None)),
            }
        }

        fn captured(&self) -> Option<(String, f64)> {
            let id = self.last_id.lock().unwrap().clone()?;
            let val = *self.last_value.lock().unwrap();
            Some((id, val?))
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
        async fn set_param(&self, param_id: &str, value: f64) -> Result<(), VTubeError> {
            *self.last_id.lock().unwrap() = Some(param_id.to_owned());
            *self.last_value.lock().unwrap() = Some(value);
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
    async fn execute_passes_interpolated_id_and_literal_value_to_sink() {
        let sink = Arc::new(CaptureSink::new());
        let runner = ParamSetRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack =
            ArgStack::new().set("pid".to_owned(), Variant::String("DynamicParam".to_owned()));
        let config = BTreeMap::from([
            ("param_id".to_owned(), Variant::String("%pid%".to_owned())),
            ("value".to_owned(), Variant::Float(0.75)),
        ]);
        let ctx = make_ctx(&stack);
        runner.execute(&config, &ctx).await;
        let (id, val) = sink
            .captured()
            .expect("sink must have been called after execute");
        assert_eq!(
            id, "DynamicParam",
            "param_id must be interpolated before passing to sink"
        );
        assert!(
            (val - 0.75).abs() < f64::EPSILON,
            "float value must be passed verbatim, not interpolated"
        );
    }

    #[test]
    fn validate_config_rejects_a_non_numeric_value() {
        let runner = ParamSetRunner::new(Arc::new(MockSink::new()));
        let config = BTreeMap::from([
            ("param_id".to_owned(), Variant::String("P".to_owned())),
            (
                "value".to_owned(),
                Variant::String("not-a-float".to_owned()),
            ),
        ]);
        assert!(runner.validate_config(&config).is_err());
    }
}
