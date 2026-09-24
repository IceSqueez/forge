use forge_registry::RunContext;
use forge_registry::runner::SubActionConfig;
use forge_types::Variant;

/// Empty text and an absent key both read as `None`; text is `%var%`-interpolated before parsing.
pub(crate) fn optional_number(
    config: &SubActionConfig,
    key: &str,
    ctx: &RunContext<'_>,
) -> Result<Option<f64>, String> {
    match config.get(key) {
        None => Ok(None),
        Some(Variant::Float(f)) => Ok(Some(*f)),
        Some(Variant::Int(i)) => Ok(Some(*i as f64)),
        Some(Variant::String(raw)) => {
            let text = ctx.arg_stack.interpolate(raw);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            trimmed
                .parse::<f64>()
                .ok()
                .filter(|f| f.is_finite())
                .map(Some)
                .ok_or_else(|| format!("'{key}' must be a number, got '{trimmed}'"))
        }
        Some(_) => Err(format!("'{key}' must be a number")),
    }
}

pub(crate) fn required_number(
    config: &SubActionConfig,
    key: &str,
    ctx: &RunContext<'_>,
) -> Result<f64, String> {
    optional_number(config, key, ctx)?.ok_or_else(|| format!("'{key}' is required"))
}

/// Empty text and an absent key both read as `None`; text is `%var%`-interpolated before parsing.
pub(crate) fn optional_integer(
    config: &SubActionConfig,
    key: &str,
    ctx: &RunContext<'_>,
) -> Result<Option<i64>, String> {
    match config.get(key) {
        None => Ok(None),
        Some(Variant::Int(i)) => Ok(Some(*i)),
        Some(Variant::String(raw)) => {
            let text = ctx.arg_stack.interpolate(raw);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            trimmed
                .parse::<i64>()
                .map(Some)
                .map_err(|_| format!("'{key}' must be a whole number, got '{trimmed}'"))
        }
        Some(_) => Err(format!("'{key}' must be a whole number")),
    }
}

/// A `%var%` template passes because its value is only known at run time.
pub(crate) fn accepts_number(value: Option<&Variant>) -> bool {
    match value {
        Some(Variant::Float(_) | Variant::Int(_)) => true,
        Some(Variant::String(s)) => s.trim().parse::<f64>().is_ok() || s.contains('%'),
        _ => false,
    }
}

