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
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::error::VTubeError;
    use crate::runners::test_support::{MockSink, make_ctx};

    const LONG_AFTER: Duration = Duration::from_secs(60);

    #[derive(Default)]
    struct ParamLog {
        sent: std::sync::Mutex<Vec<(String, f64)>>,
        failing: AtomicBool,
        rejected_value: std::sync::Mutex<Option<f64>>,
    }

    impl ParamLog {
        fn sent(&self) -> Vec<(String, f64)> {
            self.sent.lock().unwrap().clone()
        }

        fn sends_to(&self, param_id: &str) -> usize {
            self.sent().iter().filter(|(id, _)| id == param_id).count()
        }

        fn fail_from_now(&self) {
            self.failing.store(true, Ordering::SeqCst);
        }

        fn reject_value(&self, value: f64) {
            *self.rejected_value.lock().unwrap() = Some(value);
        }
    }

    #[async_trait]
    impl VTubeSink for ParamLog {
        async fn trigger_hotkey(&self, _: &str) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn set_expression(&self, _: &str, _: bool) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn set_param(&self, param_id: &str, value: f64) -> Result<(), VTubeError> {
            self.sent.lock().unwrap().push((param_id.to_owned(), value));
            if self.failing.load(Ordering::SeqCst)
                || *self.rejected_value.lock().unwrap() == Some(value)
            {
                Err(VTubeError::NotConnected)
            } else {
                Ok(())
            }
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

    fn runner_with_log() -> (ParamSetRunner, Arc<ParamLog>) {
        let log = Arc::new(ParamLog::default());
        let runner = ParamSetRunner::new(
            Arc::clone(&log) as Arc<dyn VTubeSink>,
            Arc::new(HoldRegistry::new()),
        );
        (runner, log)
    }

    fn config(param_id: &str, value: f64, hold_secs: Option<Variant>) -> SubActionConfig {
        let mut config = BTreeMap::from([
            ("param_id".to_owned(), Variant::String(param_id.to_owned())),
            ("value".to_owned(), Variant::Float(value)),
        ]);
        if let Some(hold) = hold_secs {
            config.insert("hold_secs".to_owned(), hold);
        }
        config
    }

    async fn run(runner: &ParamSetRunner, config: &SubActionConfig) -> SubActionOutcome {
        let stack = ArgStack::new();
        runner.execute(config, &make_ctx(&stack)).await.0.outcome
    }

    #[test]
    fn validate_config_accepts_valid_param() {
        let runner = ParamSetRunner::new(Arc::new(MockSink::new()), Arc::new(HoldRegistry::new()));
        assert!(
            runner
                .validate_config(&config("MyParam", 0.5, None))
                .is_ok()
        );
    }

    #[test]
    fn validate_config_rejects_missing_param_id() {
        let runner = ParamSetRunner::new(Arc::new(MockSink::new()), Arc::new(HoldRegistry::new()));
        let config = BTreeMap::from([("value".to_owned(), Variant::Float(1.0))]);
        assert!(runner.validate_config(&config).is_err());
    }

    #[test]
    fn validate_config_rejects_a_non_numeric_value() {
        let runner = ParamSetRunner::new(Arc::new(MockSink::new()), Arc::new(HoldRegistry::new()));
        let config = BTreeMap::from([
            ("param_id".to_owned(), Variant::String("P".to_owned())),
            (
                "value".to_owned(),
                Variant::String("not-a-float".to_owned()),
            ),
        ]);
        assert!(runner.validate_config(&config).is_err());
    }

    #[test]
    fn validate_config_accepts_a_numeric_blank_or_templated_hold() {
        let runner = ParamSetRunner::new(Arc::new(MockSink::new()), Arc::new(HoldRegistry::new()));
        for hold in [
            Variant::Float(2.5),
            Variant::Int(3),
            Variant::String("2.5".to_owned()),
            Variant::String(String::new()),
            Variant::String("   ".to_owned()),
            Variant::String("%hold%".to_owned()),
        ] {
            let result = runner.validate_config(&config("P", 1.0, Some(hold.clone())));
            assert!(result.is_ok(), "{hold:?} was rejected: {result:?}");
        }
    }

    #[test]
    fn validate_config_rejects_a_hold_that_is_not_a_number() {
        let runner = ParamSetRunner::new(Arc::new(MockSink::new()), Arc::new(HoldRegistry::new()));
        for hold in [Variant::String("soon".to_owned()), Variant::Bool(true)] {
            assert!(
                runner
                    .validate_config(&config("P", 1.0, Some(hold.clone())))
                    .is_err(),
                "{hold:?} was accepted"
            );
        }
    }

    #[tokio::test]
    async fn execute_passes_interpolated_id_and_literal_value_to_sink() {
        let (runner, log) = runner_with_log();
        let stack =
            ArgStack::new().set("pid".to_owned(), Variant::String("DynamicParam".to_owned()));
        runner
            .execute(&config("%pid%", 0.75, None), &make_ctx(&stack))
            .await;
        assert_eq!(log.sent(), vec![("DynamicParam".to_owned(), 0.75)]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_hold_resends_the_value_every_interval_until_the_duration_is_covered() {
        let (runner, log) = runner_with_log();
        run(&runner, &config("P", 0.5, Some(Variant::Float(1.0)))).await;

        let mut observed = Vec::new();
        let mut elapsed = Duration::ZERO;
        for at in [399, 401, 1199, 1201, 60_000].map(Duration::from_millis) {
            tokio::time::sleep(at - elapsed).await;
            elapsed = at;
            observed.push(log.sends_to("P"));
        }

        assert_eq!(observed, vec![1, 2, 3, 4, 4]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_zero_negative_or_blank_hold_sends_the_value_once() {
        for hold in [
            None,
            Some(Variant::Float(0.0)),
            Some(Variant::Float(-5.0)),
            Some(Variant::Float(f64::NAN)),
            Some(Variant::String(String::new())),
        ] {
            let (runner, log) = runner_with_log();
            run(&runner, &config("P", 0.5, hold.clone())).await;

            tokio::time::sleep(LONG_AFTER).await;

            assert_eq!(log.sends_to("P"), 1, "hold {hold:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_hold_longer_than_the_cap_stops_resending_at_the_cap() {
        let (runner, log) = runner_with_log();
        run(&runner, &config("P", 0.5, Some(Variant::Float(601.0)))).await;

        tokio::time::sleep(Duration::from_secs(700)).await;

        assert_eq!(log.sends_to("P"), 1 + 600_000 / 400);
    }

    #[tokio::test(start_paused = true)]
    async fn a_new_hold_on_the_same_parameter_replaces_the_running_one() {
        let (runner, log) = runner_with_log();
        run(&runner, &config("P", 1.0, Some(Variant::Float(10.0)))).await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let before_replacement = log.sent().len();

        run(&runner, &config("P", 2.0, Some(Variant::Float(1.0)))).await;
        tokio::time::sleep(LONG_AFTER).await;

        let after: Vec<f64> = log.sent()[before_replacement..]
            .iter()
            .map(|(_, value)| *value)
            .collect();
        assert_eq!(after, vec![2.0; 4]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_one_shot_set_on_a_held_parameter_ends_the_hold() {
        let (runner, log) = runner_with_log();
        run(&runner, &config("P", 1.0, Some(Variant::Float(10.0)))).await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let before_one_shot = log.sent().len();

        run(&runner, &config("P", 0.0, Some(Variant::Float(0.0)))).await;
        tokio::time::sleep(LONG_AFTER).await;

        let after: Vec<f64> = log.sent()[before_one_shot..]
            .iter()
            .map(|(_, value)| *value)
            .collect();
        assert_eq!(
            after,
            vec![0.0],
            "the old hold kept overwriting the one-shot value"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_set_on_a_held_parameter_still_ends_the_hold() {
        let (runner, log) = runner_with_log();
        run(&runner, &config("P", 1.0, Some(Variant::Float(10.0)))).await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
        log.reject_value(2.0);
        let before_failed_set = log.sent().len();

        let outcome = run(&runner, &config("P", 2.0, None)).await;
        tokio::time::sleep(LONG_AFTER).await;

        assert!(
            matches!(outcome, SubActionOutcome::Failed(_)),
            "got {outcome:?}"
        );
        let after: Vec<f64> = log.sent()[before_failed_set..]
            .iter()
            .map(|(_, value)| *value)
            .collect();
        assert_eq!(after, vec![2.0], "the old hold outlived the failed set");
    }

    #[tokio::test(start_paused = true)]
    async fn holds_on_different_parameters_run_side_by_side() {
        let (runner, log) = runner_with_log();
        run(&runner, &config("P", 1.0, Some(Variant::Float(1.0)))).await;
        run(&runner, &config("Q", 2.0, Some(Variant::Float(1.0)))).await;

        tokio::time::sleep(LONG_AFTER).await;

        assert_eq!((log.sends_to("P"), log.sends_to("Q")), (4, 4));
    }

    #[tokio::test(start_paused = true)]
    async fn a_hold_stops_at_the_first_failed_resend() {
        let (runner, log) = runner_with_log();
        run(&runner, &config("P", 1.0, Some(Variant::Float(10.0)))).await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let before_disconnect = log.sends_to("P");

        log.fail_from_now();
        tokio::time::sleep(LONG_AFTER).await;

        assert_eq!(log.sends_to("P"), before_disconnect + 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_first_send_reports_failure_and_starts_no_hold() {
        let (runner, log) = runner_with_log();
        log.fail_from_now();

        let outcome = run(&runner, &config("P", 1.0, Some(Variant::Float(10.0)))).await;
        tokio::time::sleep(LONG_AFTER).await;

        assert!(
            matches!(outcome, SubActionOutcome::Failed(_)),
            "got {outcome:?}"
        );
        assert_eq!(log.sends_to("P"), 1);
    }

    #[tokio::test]
    async fn a_hold_that_is_not_a_number_at_run_time_fails_without_sending() {
        let (runner, log) = runner_with_log();
        let stack = ArgStack::new().set("hold".to_owned(), Variant::String("soon".to_owned()));
        let config = config("P", 1.0, Some(Variant::String("%hold%".to_owned())));

        let (tel, _) = runner.execute(&config, &make_ctx(&stack)).await;

        assert!(
            matches!(tel.outcome, SubActionOutcome::Failed(_)),
            "got {:?}",
            tel.outcome
        );
        assert!(log.sent().is_empty());
    }
}
