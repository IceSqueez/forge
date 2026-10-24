use forge_registry::RunContext;
use forge_types::{SubActionConfig, Variant};

pub(super) fn resolve_float(
    config: &SubActionConfig,
    ctx: &RunContext<'_>,
    key: &str,
    default: f64,
) -> Result<f64, String> {
    let raw = match config.get(key) {
        Some(Variant::Float(n)) => return finite(key, *n),
        Some(Variant::Int(n)) => return Ok(*n as f64),
        Some(Variant::String(s)) => s.clone(),
        _ => return Ok(default),
    };
    if raw.trim().is_empty() {
        return Ok(default);
    }
    let resolved = ctx.arg_stack.interpolate(&raw);
    let parsed = resolved
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("{key} is not a valid number: {resolved:?}"))?;
    finite(key, parsed)
}

fn finite(key: &str, value: f64) -> Result<f64, String> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("{key} must be a finite number, got {value}"))
    }
}
