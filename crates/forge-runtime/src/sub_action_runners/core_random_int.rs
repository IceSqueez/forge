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

pub struct CoreRandomIntRunner;

fn resolve_bound(
    config: &SubActionConfig,
    ctx: &RunContext<'_>,
    key: &str,
    default: i64,
) -> Result<i64, String> {
    let raw = match config.get(key) {
        Some(Variant::Int(n)) => return Ok(*n),
        Some(Variant::String(s)) => s.clone(),
        _ => return Ok(default),
    };
    if raw.trim().is_empty() {
        return Ok(default);
    }
    let resolved = ctx.arg_stack.interpolate(&raw);
    resolved
        .trim()
        .parse::<i64>()
        .map_err(|_| format!("{key} is not a valid integer: {resolved:?}"))
}

#[async_trait]
impl SubActionRunner for CoreRandomIntRunner {
    fn id(&self) -> &str {
        "core.random.int"
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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("min".to_owned(), Variant::String("1".to_owned()));
        cfg.insert("max".to_owned(), Variant::String("100".to_owned()));
        cfg.insert("target_var".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "min",
                label: "Minimum",
                placeholder: "1",
            },
            FormField::Text {
                key: "max",
                label: "Maximum",
                placeholder: "100",
            },
            FormField::Text {
                key: "target_var",
                label: "Output Variable",
                placeholder: "random_result",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("target_var").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.random.int: target_var is required".to_owned(),
            )),
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "target_var".to_owned(),
                kind: VariantKind::Int,
                label: "Random integer".to_owned(),
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

        let min = resolve_bound(config, ctx, "min", 1);
        let max = resolve_bound(config, ctx, "max", 100);
        let target_var = super::interpolate::sanitize_var_name(
            config
                .get("target_var")
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
                let stack = ctx.arg_stack.clone().set(target_var, Variant::Int(value));
                (SubActionOutcome::Success, Some(stack))
            }
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "core.random.int".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            produced,
        )
    }
}
