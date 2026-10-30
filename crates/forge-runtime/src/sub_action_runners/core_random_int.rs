use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, IntegerEditor, IntegerField, Interpolation,
    ProducedVariable, RunContext, StepTimer, SubActionCategory, SubActionIo, SubActionRunner,
    TypedField, ValueBounds, VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};
use rand::RngExt;

pub const RANDOM_INT_KIND_ID: &str = "core.random.int";
pub const RANDOM_INT_MIN_KEY: &str = "min";
pub const RANDOM_INT_MAX_KEY: &str = "max";
pub const TARGET_VAR_KEY: &str = "target_var";

const MIN: IntegerField = IntegerField {
    key: RANDOM_INT_MIN_KEY,
    label: "Minimum",
    editor: IntegerEditor::Text { placeholder: "1" },
    default: 1,
    bounds: ValueBounds::UNBOUNDED,
};

const MAX: IntegerField = IntegerField {
    key: RANDOM_INT_MAX_KEY,
    label: "Maximum",
    editor: IntegerEditor::Text { placeholder: "100" },
    default: 100,
    bounds: ValueBounds::UNBOUNDED,
};

const TARGET_VAR: VariableField = VariableField {
    key: TARGET_VAR_KEY,
    label: "Output Variable",
    placeholder: "random_result",
    default: "",
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MIN, &MAX, &TARGET_VAR]);

fn ordered(min: i64, max: i64) -> Result<(), ConfigProblem> {
    if min <= max {
        Ok(())
    } else {
        Err(ConfigProblem::field(
            MAX.key,
            ConfigReason::OutOfRange(ValueBounds::at_least(min)),
        ))
    }
}

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(i64, i64, String), ConfigProblem> {
    let min = MIN.read(config, args)?;
    let max = MAX.read(config, args)?;
    ordered(min, max)?;
    Ok((min, max, TARGET_VAR.read(config, args)?))
}

pub struct CoreRandomIntRunner;

#[async_trait]
impl SubActionRunner for CoreRandomIntRunner {
    fn id(&self) -> &str {
        RANDOM_INT_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Random Integer"
    }

    fn summary(&self) -> &str {
        "Generate a random integer in [min, max] and store it in a variable"
    }

    fn search_text(&self) -> &str {
        "random integer number generate range"
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
            (Some(min), Some(max)) => ordered(min, max),
            _ => Ok(()),
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: TARGET_VAR_KEY.to_owned(),
                kind: VariantKind::Int,
                label: "Random integer".to_owned(),
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
        let (min, max, target_var) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let value = rand::rng().random_range(min..=max);
        let stack = ctx.arg_stack.clone().set(target_var, Variant::Int(value));

        (timer.success(), Some(stack))
    }
}

#[cfg(test)]
mod tests {
    use forge_events::{Event, EventPublisher};
    use forge_registry::RegistryError;
    use forge_types::{EventId, SubActionOutcome};

    use super::*;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn cfg(min: Variant, max: Variant) -> SubActionConfig {
        SubActionConfig::from([
            ("min".to_owned(), min),
            ("max".to_owned(), max),
            (TARGET_VAR_KEY.to_owned(), text("roll")),
        ])
    }

    fn args() -> ArgStack {
        ArgStack::new()
            .set("lo".to_owned(), Variant::Int(4))
            .set("hi".to_owned(), Variant::Int(4))
            .set("low".to_owned(), Variant::Int(2))
    }

    async fn run(cfg: &SubActionConfig) -> (SubActionOutcome, Option<ArgStack>) {
        let stack = args();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, produced) = CoreRandomIntRunner.execute(cfg, &ctx).await;
        (telemetry.outcome, produced)
    }

    fn max_below(min: i64) -> ConfigProblem {
        ConfigProblem::field("max", ConfigReason::OutOfRange(ValueBounds::at_least(min)))
    }

    #[tokio::test]
    async fn an_equal_min_and_max_produce_exactly_that_value() {
        for config in [
            cfg(text("4"), text("4")),
            cfg(Variant::Int(4), Variant::Int(4)),
            cfg(text("%lo%"), text("%hi%")),
        ] {
            let (_, produced) = run(&config).await;

            assert_eq!(
                produced.as_ref().and_then(|stack| stack.get("roll")),
                Some(&Variant::Int(4)),
                "{config:?}"
            );
        }
    }

    #[tokio::test]
    async fn every_roll_lands_inside_the_inclusive_range() {
        for _ in 0..200 {
            let (_, produced) = run(&cfg(text("-1"), text("1"))).await;
            let roll = produced.as_ref().and_then(|stack| stack.get("roll"));

            assert!(matches!(roll, Some(Variant::Int(-1..=1))), "{roll:?}");
        }
    }

    #[tokio::test]
    async fn a_min_above_max_fails_the_run_naming_max_and_produces_nothing() {
        for config in [cfg(text("5"), text("4")), cfg(text("%lo%"), text("%low%"))] {
            let (outcome, produced) = run(&config).await;

            assert!(
                matches!(&outcome, SubActionOutcome::Failed(message) if message.code_or_text().contains("'max'")),
                "{config:?}: {outcome:?}"
            );
            assert!(produced.is_none(), "{config:?}");
        }
    }

    #[test]
    fn check_config_refuses_literal_bounds_out_of_order_and_admits_equal_ones() {
        for (min, max, expected) in [
            (text("5"), text("4"), Err(max_below(5))),
            (text("4"), text("4"), Ok(())),
            (Variant::Int(-3), Variant::Int(-4), Err(max_below(-3))),
        ] {
            let config = cfg(min, max);

            assert_eq!(
                CoreRandomIntRunner.check_config(&config, Interpolation::Deferred),
                expected,
                "{config:?}"
            );
        }
    }

    #[test]
    fn check_config_defers_a_templated_bound_until_the_run_supplies_it() {
        let config = cfg(text("%lo%"), text("%low%"));
        let args = args();

        assert_eq!(
            (
                CoreRandomIntRunner.check_config(&config, Interpolation::Deferred),
                CoreRandomIntRunner.check_config(&config, Interpolation::Against(&args)),
            ),
            (Ok(()), Err(max_below(4)))
        );
    }

    #[test]
    fn validate_config_wraps_an_out_of_order_range_as_invalid_config() {
        let result = CoreRandomIntRunner.validate_config(&cfg(text("9"), text("1")));

        assert!(
            matches!(&result, Err(RegistryError::InvalidConfig(problem)) if *problem == max_below(9)),
            "{result:?}"
        );
    }
}
