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

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::{ArgStack, EventId};

    const KEY: &str = "probability_true";
    const DEFAULT: f64 = 0.5;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn resolve(value: Option<Variant>) -> Result<f64, String> {
        let mut config = SubActionConfig::new();
        if let Some(value) = value {
            config.insert(KEY.to_owned(), value);
        }
        let stack = ArgStack::new().set("p".to_owned(), Variant::String("0.75".to_owned()));
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        resolve_float(&config, &ctx, KEY, DEFAULT)
    }

    fn text(s: &str) -> Option<Variant> {
        Some(Variant::String(s.to_owned()))
    }

    #[test]
    fn accepted_inputs_resolve_to_their_number() {
        for (input, expected) in [
            (text("1"), 1.0),
            (text("0"), 0.0),
            (text("0.25"), 0.25),
            (text(" 0.25 "), 0.25),
            (text("1.5"), 1.5),
            (text("-0.1"), -0.1),
            (text("%p%"), 0.75),
            (text(""), DEFAULT),
            (text("   "), DEFAULT),
            (None, DEFAULT),
            (Some(Variant::Bool(true)), DEFAULT),
            (Some(Variant::Float(0.3)), 0.3),
            (Some(Variant::Int(1)), 1.0),
        ] {
            assert_eq!(resolve(input.clone()).unwrap(), expected, "{input:?}");
        }
    }

    #[test]
    fn rejected_inputs_fail_naming_the_field() {
        for input in [
            text("abc"),
            text("%missing%"),
            text("NaN"),
            text("inf"),
            Some(Variant::Float(f64::NAN)),
            Some(Variant::Float(f64::NEG_INFINITY)),
        ] {
            let err = resolve(input.clone()).unwrap_err();
            assert!(err.starts_with(KEY), "{input:?}: {err}");
        }
    }
}
