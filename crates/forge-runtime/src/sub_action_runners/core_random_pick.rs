use crate::core_messages;
use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, Interpolation, LinesField, ProducedVariable,
    RunContext, StepTimer, SubActionCategory, SubActionIo, SubActionRunner, TypedField,
    VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};
use rand::RngExt;
use rand::distr::Distribution;
use rand::distr::weighted::WeightedIndex;

const INTO_VAR_KEY: &str = "into_var";
const PICKED_VARIABLE: &str = "picked";

const ITEMS: LinesField = LinesField {
    key: "items",
    label: "Items (one per line)",
};

const WEIGHTS: LinesField = LinesField {
    key: "weights",
    label: "Weights (one per line, empty = uniform)",
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: PICKED_VARIABLE,
    default: PICKED_VARIABLE,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&ITEMS, &WEIGHTS, &INTO_VAR]);

fn written_entries(entries: Vec<Variant>) -> Vec<Variant> {
    entries
        .into_iter()
        .filter(|entry| match entry {
            Variant::String(text) => !text.trim().is_empty(),
            _ => true,
        })
        .collect()
}

fn weight_values(entries: &[Variant]) -> Result<Vec<f64>, ConfigProblem> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            match entry {
                Variant::Float(weight) => Some(*weight),
                Variant::Int(weight) => Some(*weight as f64),
                Variant::String(text) => text.trim().parse::<f64>().ok(),
                _ => None,
            }
            .ok_or_else(|| {
                ConfigProblem::field(
                    WEIGHTS.key,
                    ConfigReason::Coded(core_messages::weight_not_a_number(index + 1)),
                )
            })
        })
        .collect()
}

fn weighted_choice(
    item_count: usize,
    weights: &[f64],
) -> Result<Option<WeightedIndex<f64>>, ConfigProblem> {
    if item_count == 0 {
        return Err(ConfigProblem::field(ITEMS.key, ConfigReason::Required));
    }
    if weights.is_empty() {
        return Ok(None);
    }
    if weights.len() != item_count {
        return Err(ConfigProblem::field(
            WEIGHTS.key,
            ConfigReason::Coded(core_messages::weight_count_mismatch(
                weights.len(),
                item_count,
            )),
        ));
    }
    WeightedIndex::new(weights).map(Some).map_err(|error| {
        ConfigProblem::field(
            WEIGHTS.key,
            ConfigReason::Coded(core_messages::weights_unusable(error)),
        )
    })
}

struct PickConfig {
    items: Vec<Variant>,
    weights: Option<WeightedIndex<f64>>,
    into_var: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<PickConfig, ConfigProblem> {
    let items: Vec<Variant> = written_entries(ITEMS.read(config, args)?)
        .into_iter()
        .map(|item| match item {
            Variant::String(line) => Variant::String(args.interpolate(line.trim())),
            other => other,
        })
        .collect();
    let weights = weight_values(&written_entries(WEIGHTS.read(config, args)?))?;
    Ok(PickConfig {
        weights: weighted_choice(items.len(), &weights)?,
        items,
        into_var: INTO_VAR.read(config, args)?,
    })
}

pub struct CoreRandomPickRunner;

#[async_trait]
impl SubActionRunner for CoreRandomPickRunner {
    fn id(&self) -> &str {
        "core.random.pick"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Random Pick"
    }

    fn summary(&self) -> &str {
        "Pick a random element from a list and store it in a variable"
    }

    fn search_text(&self) -> &str {
        "random pick choose select list element weighted"
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
        let (Some(items), Some(weights)) = (
            ITEMS.peek(config, interpolation)?,
            WEIGHTS.peek(config, interpolation)?,
        ) else {
            return Ok(());
        };
        let weights = weight_values(&written_entries(weights))?;
        weighted_choice(written_entries(items).len(), &weights).map(|_| ())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::String,
                label: "Picked item".to_owned(),
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
        let PickConfig {
            mut items,
            weights,
            into_var,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let index = match weights {
            Some(distribution) => distribution.sample(&mut rand::rng()),
            None => rand::rng().random_range(0..items.len()),
        };
        let picked = items.swap_remove(index);
        let stack = ctx.arg_stack.clone().set(into_var, picked);

        (timer.success(), Some(stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::test_support::NullPublisher;

    use forge_types::EventId;
    use forge_types::SubActionOutcome;

    fn str_array(items: &[&str]) -> Variant {
        Variant::Array(
            items
                .iter()
                .map(|s| Variant::String((*s).to_owned()))
                .collect(),
        )
    }

    fn cfg(items: Variant, into: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("items".to_owned(), items);
        c.insert("into_var".to_owned(), Variant::String(into.to_owned()));
        c
    }

    async fn run(
        runner: &CoreRandomPickRunner,
        cfg: &SubActionConfig,
    ) -> (SubActionOutcome, Option<ArgStack>) {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, produced) = runner.execute(cfg, &ctx).await;
        (telemetry.outcome, produced)
    }

    #[tokio::test]
    async fn pick_produces_member_of_source_list() {
        let cases = [
            (str_array(&["a", "b", "c"]), ["a", "b", "c"]),
            (Variant::String("x\ny\nz".to_owned()), ["x", "y", "z"]),
        ];
        for (items, expected_set) in cases {
            let runner = CoreRandomPickRunner;
            let cfg = cfg(items, "chosen");
            for _ in 0..200 {
                let (outcome, produced) = run(&runner, &cfg).await;
                assert!(matches!(outcome, SubActionOutcome::Success));
                let picked = produced.unwrap().get("chosen").unwrap().clone();
                assert!(
                    matches!(&picked, Variant::String(s) if expected_set.contains(&s.as_str())),
                    "picked outside source set: {picked:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn pick_respects_zero_weight_never_selecting_it() {
        let runner = CoreRandomPickRunner;
        let mut cfg = cfg(str_array(&["never", "always"]), "chosen");
        cfg.insert(
            "weights".to_owned(),
            Variant::Array(vec![Variant::Float(0.0), Variant::Float(1.0)]),
        );
        for _ in 0..100 {
            let (outcome, produced) = run(&runner, &cfg).await;
            assert!(matches!(outcome, SubActionOutcome::Success));
            assert_eq!(
                produced.unwrap().get("chosen"),
                Some(&Variant::String("always".to_owned()))
            );
        }
    }

    #[tokio::test]
    async fn pick_with_a_blank_target_var_fails_instead_of_writing_a_default_name() {
        let runner = CoreRandomPickRunner;
        let (outcome, produced) = run(&runner, &cfg(str_array(&["only"]), "")).await;
        assert!(
            matches!(&outcome, SubActionOutcome::Failed(m) if crate::test_support::failure::is_field_problem(m, "into_var", "config.required")),
            "{outcome:?}"
        );
        assert!(produced.is_none());
    }

    #[tokio::test]
    async fn pick_failure_paths_produce_no_stack() {
        let empty = cfg(Variant::Array(vec![]), "chosen");

        let mut mismatch = cfg(str_array(&["a", "b"]), "chosen");
        mismatch.insert(
            "weights".to_owned(),
            Variant::Array(vec![Variant::Float(1.0)]),
        );

        let mut all_zero = cfg(str_array(&["a", "b"]), "chosen");
        all_zero.insert(
            "weights".to_owned(),
            Variant::Array(vec![Variant::Float(0.0), Variant::Float(0.0)]),
        );

        let runner = CoreRandomPickRunner;
        for (label, cfg) in [
            ("empty items", empty),
            ("weights length mismatch", mismatch),
            ("all-zero weights", all_zero),
        ] {
            let (outcome, produced) = run(&runner, &cfg).await;
            assert!(
                matches!(outcome, SubActionOutcome::Failed(_)),
                "{label} must fail"
            );
            assert!(produced.is_none(), "{label} must not produce a scope stack");
        }
    }

    #[tokio::test]
    async fn pick_rejects_a_non_numeric_weight_naming_its_position() {
        let mut text_weights = cfg(Variant::String("a\nb\nc".to_owned()), "chosen");
        text_weights.insert(
            "weights".to_owned(),
            Variant::String("1\n\n  abc  \n2".to_owned()),
        );

        let mut array_weights = cfg(str_array(&["a", "b", "c"]), "chosen");
        array_weights.insert(
            "weights".to_owned(),
            Variant::Array(vec![
                Variant::Int(1),
                Variant::String("abc".to_owned()),
                Variant::Int(2),
            ]),
        );

        let runner = CoreRandomPickRunner;
        for (label, cfg) in [("text", text_weights), ("array", array_weights)] {
            let (outcome, produced) = run(&runner, &cfg).await;
            assert_eq!(
                outcome,
                SubActionOutcome::Failed(crate::test_support::failure::step_problem(
                    "core.random.pick",
                    &ConfigProblem::field(
                        "weights",
                        ConfigReason::Coded(crate::core_messages::weight_not_a_number(2))
                    )
                )),
                "{label}"
            );
            assert!(produced.is_none(), "{label}");
        }
    }

    #[tokio::test]
    async fn pick_accepts_integer_weights_in_an_array() {
        let runner = CoreRandomPickRunner;
        let mut cfg = cfg(str_array(&["never", "always"]), "chosen");
        cfg.insert(
            "weights".to_owned(),
            Variant::Array(vec![Variant::Int(0), Variant::Int(3)]),
        );
        for _ in 0..50 {
            let (outcome, produced) = run(&runner, &cfg).await;
            assert!(matches!(outcome, SubActionOutcome::Success), "{outcome:?}");
            assert_eq!(
                produced.unwrap().get("chosen"),
                Some(&Variant::String("always".to_owned()))
            );
        }
    }

    #[tokio::test]
    async fn pick_text_items_are_trimmed_and_interpolated() {
        let runner = CoreRandomPickRunner;
        let stack = ArgStack::new().set("name".to_owned(), Variant::String("Ann".to_owned()));
        for (weights, expected) in [("1\n0", "hi Ann"), ("0\n1", "bye")] {
            let mut cfg = cfg(
                Variant::String("  hi %name%  \n\n\t bye \t".to_owned()),
                "chosen",
            );
            cfg.insert("weights".to_owned(), Variant::String(weights.to_owned()));
            let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
            let (telemetry, produced) = runner.execute(&cfg, &ctx).await;
            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Success),
                "{weights:?}: {:?}",
                telemetry.outcome
            );
            assert_eq!(
                produced.unwrap().get("chosen"),
                Some(&Variant::String(expected.to_owned())),
                "{weights:?}"
            );
        }
    }
}
