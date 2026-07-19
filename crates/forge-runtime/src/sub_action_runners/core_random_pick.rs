use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, SubActionCategory, SubActionIo,
    SubActionRunner,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};
use rand::RngExt;
use rand::distr::Distribution;
use rand::distr::weighted::WeightedIndex;
use time::OffsetDateTime;

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("items".to_owned(), Variant::Array(vec![]));
        cfg.insert("weights".to_owned(), Variant::Array(vec![]));
        cfg.insert("into_var".to_owned(), Variant::String("picked".to_owned()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "items",
                label: "Items (one per line)",
            },
            FormField::TextArea {
                key: "weights",
                label: "Weights (one per line, empty = uniform)",
            },
            FormField::Text {
                key: "into_var",
                label: "Target Variable",
                placeholder: "picked",
            },
        ]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::String,
                label: "Picked item".to_owned(),
            }],
            consumes: Vec::new(),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let items: Vec<Variant> = match config.get("items") {
            Some(Variant::Array(arr)) => arr.clone(),
            Some(v) => v
                .as_str()
                .unwrap_or("")
                .lines()
                .filter(|s| !s.is_empty())
                .map(|s| Variant::String(s.to_owned()))
                .collect(),
            None => vec![],
        };

        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("picked")
            .to_owned();

        let (outcome, produced) = if items.is_empty() {
            (
                SubActionOutcome::Failed("items list must not be empty".to_owned()),
                None,
            )
        } else {
            let weights: Vec<f64> = match config.get("weights") {
                Some(Variant::Array(arr)) => arr.iter().filter_map(|v| v.as_float()).collect(),
                Some(v) => v
                    .as_str()
                    .unwrap_or("")
                    .lines()
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.trim().parse::<f64>().ok())
                    .collect(),
                None => vec![],
            };

            let idx = if weights.is_empty() {
                Ok(rand::rng().random_range(0..items.len()))
            } else if weights.len() != items.len() {
                Err(format!(
                    "weights length ({}) must match items length ({})",
                    weights.len(),
                    items.len()
                ))
            } else {
                WeightedIndex::new(&weights)
                    .map(|dist| dist.sample(&mut rand::rng()))
                    .map_err(|e| format!("invalid weights: {e}"))
            };

            match idx {
                Err(msg) => (SubActionOutcome::Failed(msg), None),
                Ok(i) => {
                    let picked = items[i].clone();
                    let stack = ctx.arg_stack.clone().set(into_var, picked);
                    (SubActionOutcome::Success, Some(stack))
                }
            }
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.random.pick".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            produced,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

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
        // Both the array form and the newline-delimited string form must yield
        // an element of the source set, bound to the target var in the scope.
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
    async fn pick_defaults_target_var_to_picked_when_blank() {
        let runner = CoreRandomPickRunner;
        let (outcome, produced) = run(&runner, &cfg(str_array(&["only"]), "")).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert!(produced.unwrap().get("picked").is_some());
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
}
