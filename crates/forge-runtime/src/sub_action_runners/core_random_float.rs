use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, SubActionCategory, SubActionIo,
    SubActionRunner,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};
use rand::RngExt;
use time::OffsetDateTime;

pub struct CoreRandomFloatRunner;

fn resolve_bound(
    config: &SubActionConfig,
    ctx: &RunContext<'_>,
    key: &str,
    default: f64,
) -> Result<f64, String> {
    let raw = match config.get(key) {
        Some(Variant::Float(n)) => return Ok(*n),
        Some(Variant::Int(n)) => return Ok(*n as f64),
        Some(Variant::String(s)) => s.clone(),
        _ => return Ok(default),
    };
    if raw.trim().is_empty() {
        return Ok(default);
    }
    let resolved = ctx.arg_stack.interpolate(&raw);
    resolved
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("{key} is not a valid number: {resolved:?}"))
}

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("min".to_owned(), Variant::String("0.0".to_owned()));
        cfg.insert("max".to_owned(), Variant::String("1.0".to_owned()));
        cfg.insert("into_var".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "min",
                label: "Minimum",
                placeholder: "0.0",
            },
            FormField::Text {
                key: "max",
                label: "Maximum",
                placeholder: "1.0",
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
                "core.random.float: into_var is required".to_owned(),
            )),
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::Float,
                label: "Random float".to_owned(),
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

        let min = resolve_bound(config, ctx, "min", 0.0);
        let max = resolve_bound(config, ctx, "max", 1.0);
        let into_var = super::interpolate::sanitize_var_name(
            config
                .get("into_var")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );

        let (outcome, produced) = match (min, max) {
            (Err(e), _) | (Ok(_), Err(e)) => (SubActionOutcome::Failed(e), None),
            (Ok(min), Ok(max)) if min > max => (
                SubActionOutcome::Failed(format!("min ({min}) must be <= max ({max})")),
                None,
            ),
            (Ok(min), Ok(max)) => {
                let value = rand::rng().random_range(min..=max);
                let stack = ctx.arg_stack.clone().set(into_var, Variant::Float(value));
                (SubActionOutcome::Success, Some(stack))
            }
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.random.float".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            produced,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

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
    async fn float_min_equals_max_yields_that_exact_value() {
        let (_outcome, produced) = run(&cfg(3.5, 3.5, "r")).await;
        assert_eq!(produced.unwrap().get("r"), Some(&Variant::Float(3.5)));
    }

    #[tokio::test]
    async fn float_failure_paths_produce_no_scope_stack() {
        let cases = [
            ("min greater than max", cfg(5.0, 1.0, "r")),
            (
                "unparseable min",
                cfg_v(Variant::String("abc".to_owned()), Variant::Float(1.0), "r"),
            ),
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
