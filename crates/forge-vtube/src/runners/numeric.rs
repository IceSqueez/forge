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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::ArgStack;

    use super::*;
    use crate::runners::test_support::make_ctx;

    fn read(value: Variant) -> Result<Option<f64>, String> {
        let config = BTreeMap::from([("k".to_owned(), value)]);
        optional_number(&config, "k", &make_ctx(&ArgStack::new()))
    }

    fn read_integer(value: Variant) -> Result<Option<i64>, String> {
        let config = BTreeMap::from([("k".to_owned(), value)]);
        optional_integer(&config, "k", &make_ctx(&ArgStack::new()))
    }

    fn text(s: &str) -> Variant {
        Variant::String(s.to_owned())
    }

    #[test]
    fn optional_number_reads_typed_and_padded_values() {
        for (value, expected) in [
            (Variant::Float(-0.25), Some(-0.25)),
            (Variant::Int(3), Some(3.0)),
            (text(" 0.5 "), Some(0.5)),
            (text("-1e2"), Some(-100.0)),
            (text("   "), None),
        ] {
            assert_eq!(read(value.clone()).unwrap(), expected, "{value:?}");
        }
    }

    #[test]
    fn optional_number_rejects_values_vts_cannot_take() {
        for value in [text("NaN"), text("inf"), text("0.5.1"), Variant::Bool(true)] {
            let err = read(value.clone()).unwrap_err();
            assert!(err.contains("'k'"), "{value:?} gave {err:?}");
        }
    }

    #[test]
    fn optional_integer_rejects_a_fractional_value() {
        assert_eq!(read_integer(text(" 7 ")).unwrap(), Some(7));
        assert!(read_integer(text("2.5")).is_err());
    }

    #[test]
    fn accepts_number_lets_templates_through_for_run_time_resolution() {
        for (value, expected) in [
            (Some(Variant::Float(0.5)), true),
            (Some(Variant::Int(1)), true),
            (Some(text("0.5")), true),
            (Some(text("%level%")), true),
            (Some(text("abc")), false),
            (Some(Variant::Bool(true)), false),
            (None, false),
        ] {
            assert_eq!(accepts_number(value.as_ref()), expected, "{value:?}");
        }
    }
}
