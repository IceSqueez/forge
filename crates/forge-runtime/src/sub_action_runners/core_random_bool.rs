use std::ops::RangeInclusive;

use async_trait::async_trait;
use forge_registry::{
    ConfigReason, FormField, ProducedVariable, RegistryError, RunContext, StepTimer,
    SubActionCategory, SubActionConfigExt, SubActionIo, SubActionRunner, ValueBounds,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

use super::float_input::resolve_float;

const PROBABILITY_KEY: &str = "probability_true";
const DEFAULT_PROBABILITY: f64 = 0.5;
const PROBABILITY_RANGE: RangeInclusive<f64> = 0.0..=1.0;

pub struct CoreRandomBoolRunner;

fn out_of_range() -> RegistryError {
    RegistryError::invalid_field(
        PROBABILITY_KEY,
        ConfigReason::OutOfRange(ValueBounds::between(
            *PROBABILITY_RANGE.start() as i64,
            *PROBABILITY_RANGE.end() as i64,
        )),
    )
}

fn literal_probability(config: &SubActionConfig) -> Result<Option<f64>, RegistryError> {
    match config.get(PROBABILITY_KEY) {
        Some(Variant::Float(n)) => Ok(Some(*n)),
        Some(Variant::Int(n)) => Ok(Some(*n as f64)),
        Some(Variant::String(s)) if s.contains('%') || s.trim().is_empty() => Ok(None),
        Some(Variant::String(s)) => {
            s.trim().parse::<f64>().map(Some).map_err(|_| {
                RegistryError::invalid_field(PROBABILITY_KEY, ConfigReason::NotANumber)
            })
        }
        _ => Ok(None),
    }
}

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            PROBABILITY_KEY.to_owned(),
            Variant::String(DEFAULT_PROBABILITY.to_string()),
        );
        cfg.insert("into_var".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: PROBABILITY_KEY,
                label: "Probability of True (0..1)",
                placeholder: "0.5",
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "random_result",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("into_var")?;
        match literal_probability(config)? {
            Some(p) if !PROBABILITY_RANGE.contains(&p) => Err(out_of_range()),
            _ => Ok(()),
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
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
        let timer = StepTimer::start(ctx, "core.random.bool");

        let probability = match resolve_float(config, ctx, PROBABILITY_KEY, DEFAULT_PROBABILITY) {
            Ok(p) if PROBABILITY_RANGE.contains(&p) => p,
            Ok(p) => {
                return (
                    timer.failed(format!(
                        "{PROBABILITY_KEY} must be between {} and {}, got {p}",
                        PROBABILITY_RANGE.start(),
                        PROBABILITY_RANGE.end()
                    )),
                    None,
                );
            }
            Err(e) => return (timer.failed(e), None),
        };

        let into_var =
            forge_types::strip_var_decoration(config.str("into_var").unwrap_or_default());

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
            (text(""), true),
            (text("%p%"), true),
            (Variant::Float(0.5), true),
            (text("1.5"), false),
            (text("-0.1"), false),
            (text("abc"), false),
            (Variant::Float(2.0), false),
        ] {
            let result = CoreRandomBoolRunner.validate_config(&cfg(probability.clone(), "flag"));
            assert_eq!(result.is_ok(), accepted, "{probability:?}: {result:?}");
        }
    }
}
