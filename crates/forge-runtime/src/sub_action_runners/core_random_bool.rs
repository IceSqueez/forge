use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, SubActionCategory, SubActionIo,
    SubActionRunner,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};
use time::OffsetDateTime;

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("probability_true".to_owned(), Variant::Float(0.5));
        cfg.insert("into_var".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "probability_true",
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
        match config.get("into_var").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.random.bool: into_var is required".to_owned(),
            )),
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::Bool,
                label: "Random boolean".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let probability = config
            .get("probability_true")
            .and_then(|v| v.as_float())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);

        let into_var = super::interpolate::sanitize_var_name(
            config
                .get("into_var")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );

        let value = rand::random_bool(probability);
        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::Bool(value));

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "core.random.bool".to_owned(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Success,
            },
            Some(new_stack),
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

    fn cfg(probability_true: f64, into: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert(
            "probability_true".to_owned(),
            Variant::Float(probability_true),
        );
        c.insert("into_var".to_owned(), Variant::String(into.to_owned()));
        c
    }

    async fn run(
        runner: &CoreRandomBoolRunner,
        cfg: &SubActionConfig,
    ) -> (SubActionOutcome, Option<ArgStack>) {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, produced) = runner.execute(cfg, &ctx).await;
        (telemetry.outcome, produced)
    }

    #[tokio::test]
    async fn bool_probability_edges_produce_deterministic_clamped_value() {
        // 1.0 / 0.0 are the in-range deterministic boundaries; 2.0 / -1.0 are
        // out of range and would panic in `random_bool` without the clamp. The
        // produced value now lives in the per-run scope, not GlobalsRepo.
        for (probability, expected) in [(1.0, true), (0.0, false), (2.0, true), (-1.0, false)] {
            let runner = CoreRandomBoolRunner;
            let cfg = cfg(probability, "flag");
            for _ in 0..50 {
                let (outcome, produced) = run(&runner, &cfg).await;
                assert!(matches!(outcome, SubActionOutcome::Success));
                let stack = produced.unwrap();
                assert_eq!(
                    stack.get("flag"),
                    Some(&Variant::Bool(expected)),
                    "probability={probability}"
                );
            }
        }
    }
}
