use crate::core_messages;
use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, Interpolation, NumberField, ProducedVariable,
    RunContext, StepTimer, StoredNumber, SubActionCategory, SubActionIo, SubActionRunner,
    TypedField, ValueBounds, VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};
use rand::RngExt;

const INTO_VAR_KEY: &str = "into_var";

const MIN: NumberField = NumberField {
    key: "min",
    label: "Minimum",
    placeholder: "0.0",
    default: StoredNumber::Text("0.0"),
    bounds: ValueBounds::UNBOUNDED,
};

const MAX: NumberField = NumberField {
    key: "max",
    label: "Maximum",
    placeholder: "1.0",
    default: StoredNumber::Text("1.0"),
    bounds: ValueBounds::UNBOUNDED,
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: "random_result",
    default: "",
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MIN, &MAX, &INTO_VAR]);

fn sampleable(min: f64, max: f64) -> Result<(), ConfigProblem> {
    if min > max {
        return Err(ConfigProblem::field(
            MAX.key,
            ConfigReason::Coded(core_messages::random_max_below_min()),
        ));
    }
    if (max - min).is_finite() {
        Ok(())
    } else {
        Err(ConfigProblem::field(
            MAX.key,
            ConfigReason::Coded(core_messages::random_range_too_wide()),
        ))
    }
}

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(f64, f64, String), ConfigProblem> {
    let min = MIN.read(config, args)?;
    let max = MAX.read(config, args)?;
    sampleable(min, max)?;
    Ok((min, max, INTO_VAR.read(config, args)?))
}

pub struct CoreRandomFloatRunner;

#[async_trait]
impl SubActionRunner for CoreRandomFloatRunner {
    fn id(&self) -> &str {
        "core.random.float"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Random Float"
    }

    fn summary(&self) -> &str {
        "Generate a random float in [min, max] and store it in a variable"
    }

    fn search_text(&self) -> &str {
        "random float decimal number generate range"
    }

    fn icon_name(&self) -> &str {
        "dice"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        match (
            MIN.peek(config, interpolation)?,
            MAX.peek(config, interpolation)?,
        ) {
            (Some(min), Some(max)) => sampleable(min, max),
            _ => Ok(()),
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::Float,
                label: "Random float".to_owned(),
            }],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (min, max, into_var) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let value = rand::rng().random_range(min..=max);
        let stack = ctx.arg_stack.clone().set(into_var, Variant::Float(value));

        (timer.success(), Some(stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::test_support::NullPublisher;

    use forge_types::EventId;
    use forge_types::SubActionOutcome;

    fn cfg_v(min: Variant, max: Variant, into: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("min".to_owned(), min);
        c.insert("max".to_owned(), max);
        c.insert("into_var".to_owned(), Variant::String(into.to_owned()));
        c
    }

    fn cfg(min: f64, max: f64, into: &str) -> SubActionConfig {
        cfg_v(Variant::Float(min), Variant::Float(max), into)
    }

    async fn run(cfg: &SubActionConfig) -> (SubActionOutcome, Option<ArgStack>) {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, produced) = CoreRandomFloatRunner.execute(cfg, &ctx).await;
        (telemetry.outcome, produced)
    }

    #[tokio::test]
    async fn float_result_always_lands_within_inclusive_bounds() {
        let cfg = cfg(2.0, 5.0, "r");
        for _ in 0..200 {
            let (outcome, produced) = run(&cfg).await;
            assert!(matches!(outcome, SubActionOutcome::Success));
            match produced.unwrap().get("r") {
                Some(Variant::Float(f)) => {
                    assert!((2.0..=5.0).contains(f), "out of bounds: {f}")
                }
                other => panic!("expected Float in scope, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn float_non_finite_bound_fails_naming_the_bound() {
        for (key, cfg) in [
            ("min", cfg(f64::NAN, 1.0, "r")),
            (
                "max",
                cfg_v(Variant::Float(0.0), Variant::String("inf".to_owned()), "r"),
            ),
        ] {
            let (outcome, produced) = run(&cfg).await;
            assert!(produced.is_none(), "{key}: must not produce a scope stack");
            assert!(
                matches!(&outcome, SubActionOutcome::Failed(m) if crate::test_support::failure::is_field_problem(m, key, "config.not_a_number")),
                "{key}: got {outcome:?}"
            );
        }
    }

    #[tokio::test]
    async fn float_span_of_exactly_f64_max_still_samples_within_bounds() {
        let half = f64::MAX / 2.0;
        let (outcome, produced) = run(&cfg(-half, half, "r")).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        match produced.unwrap().get("r") {
            Some(Variant::Float(f)) => assert!((-half..=half).contains(f), "out of bounds: {f}"),
            other => panic!("expected Float in scope, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn float_min_equals_max_yields_that_exact_value() {
        let (_outcome, produced) = run(&cfg(3.5, 3.5, "r")).await;
        assert_eq!(produced.unwrap().get("r"), Some(&Variant::Float(3.5)));
    }

    #[tokio::test]
    async fn float_failure_paths_fail_without_panic_or_scope_stack() {
        let cases = [
            ("min greater than max", cfg(5.0, 1.0, "r")),
            (
                "unparseable min",
                cfg_v(Variant::String("abc".to_owned()), Variant::Float(1.0), "r"),
            ),
            ("span overflowing f64", cfg(-1e308, 1e308, "r")),
        ];
        for (label, cfg) in cases {
            let (outcome, produced) = run(&cfg).await;
            assert!(
                matches!(outcome, SubActionOutcome::Failed(_)),
                "{label} must fail"
            );
            assert!(produced.is_none(), "{label} must not produce a scope stack");
        }
    }
}
