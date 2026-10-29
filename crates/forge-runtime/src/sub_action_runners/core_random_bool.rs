use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, NumberField, ProducedVariable, RunContext, StepTimer, StoredNumber,
    SubActionCategory, SubActionIo, SubActionRunner, TypedField, ValueBounds, VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

const PROBABILITY_KEY: &str = "probability_true";
const INTO_VAR_KEY: &str = "into_var";

const PROBABILITY: NumberField = NumberField {
    key: PROBABILITY_KEY,
    label: "Probability of True (0..1)",
    placeholder: "0.5",
    default: StoredNumber::Text("0.5"),
    bounds: ValueBounds {
        min: Some(0),
        max: Some(1),
    },
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: "random_result",
    default: "",
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&PROBABILITY, &INTO_VAR]);

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<(f64, String), ConfigProblem> {
    Ok((
        PROBABILITY.read(config, args)?,
        INTO_VAR.read(config, args)?,
    ))
}

pub struct CoreRandomBoolRunner;

#[async_trait]
impl SubActionRunner for CoreRandomBoolRunner {
    fn id(&self) -> &str {
        "core.random.bool"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Random Bool (Weighted)"
    }

    fn summary(&self) -> &str {
        "Generate a weighted random boolean and store it in a variable"
    }

    fn search_text(&self) -> &str {
        "random bool boolean weighted probability chance true false"
    }

    fn icon_name(&self) -> &str {
        "dice"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::Bool,
                label: "Random boolean".to_owned(),
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
        let (probability, into_var) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let value = rand::random_bool(probability);
        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::Bool(value));

        (timer.success(), Some(new_stack))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, SubActionOutcome};

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn cfg(probability_true: Variant, into: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert(PROBABILITY_KEY.to_owned(), probability_true);
        c.insert("into_var".to_owned(), Variant::String(into.to_owned()));
        c
    }

    fn text(s: &str) -> Variant {
        Variant::String(s.to_owned())
    }

    async fn run(
        runner: &CoreRandomBoolRunner,
        cfg: &SubActionConfig,
    ) -> (SubActionOutcome, Option<ArgStack>) {
        let stack = ArgStack::new().set("p".to_owned(), text("1"));
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, produced) = runner.execute(cfg, &ctx).await;
        (telemetry.outcome, produced)
    }

    #[tokio::test]
    async fn bool_probability_edges_produce_deterministic_value() {
        for (probability, expected) in [
            (text("1"), true),
            (text("0"), false),
            (text("%p%"), true),
            (Variant::Float(1.0), true),
            (Variant::Int(0), false),
        ] {
            let cfg = cfg(probability.clone(), "flag");
            for _ in 0..50 {
                let (outcome, produced) = run(&CoreRandomBoolRunner, &cfg).await;
                assert!(matches!(outcome, SubActionOutcome::Success));
                assert_eq!(
                    produced.unwrap().get("flag"),
                    Some(&Variant::Bool(expected)),
                    "probability={probability:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn bool_probability_outside_unit_range_fails_naming_the_field() {
        for probability in [
            text("1.5"),
            text("-0.1"),
            text("NaN"),
            text("abc"),
            Variant::Float(2.0),
            Variant::Int(-1),
        ] {
            let (outcome, produced) =
                run(&CoreRandomBoolRunner, &cfg(probability.clone(), "flag")).await;
            assert!(produced.is_none(), "{probability:?}");
            assert!(
                matches!(&outcome, SubActionOutcome::Failed(m) if m.contains(PROBABILITY_KEY)),
                "{probability:?}: {outcome:?}"
            );
        }
    }

    #[test]
    fn validate_config_checks_literal_probabilities_only() {
        for (probability, accepted) in [
            (text("0"), true),
            (text("1"), true),
            (text("0.25"), true),
            (text("%p%"), true),
            (Variant::Float(0.5), true),
            (text(""), false),
            (text("1.5"), false),
            (text("-0.1"), false),
            (text("abc"), false),
            (Variant::Float(2.0), false),
        ] {
            let result = CoreRandomBoolRunner.validate_config(&cfg(probability.clone(), "flag"));
            assert_eq!(result.is_ok(), accepted, "{probability:?}: {result:?}");
        }
    }

    #[test]
    fn validate_config_requires_an_output_variable_name() {
        for into in ["", "%%", "  "] {
            let result = CoreRandomBoolRunner.validate_config(&cfg(text("0.5"), into));

            assert!(
                matches!(
                    &result,
                    Err(forge_registry::RegistryError::InvalidConfig(problem))
                        if problem.field.as_deref() == Some(INTO_VAR_KEY)
                ),
                "{into:?}: {result:?}"
            );
        }
    }
}
