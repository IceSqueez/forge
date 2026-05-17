use forge_storage::{DataProvider, GlobalsRepo};
use forge_types::{ArgStack, SubActionOutcome, SubActionSpec, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub(super) async fn run(
    spec: &SubActionSpec,
    arg_stack: &ArgStack,
    index: usize,
    dp: &dyn DataProvider,
) -> SubActionTelemetry {
    let started_at = OffsetDateTime::now_utc();

    let SubActionSpec::SetGlobal { name, value } = spec else {
        unreachable!()
    };

    let name = arg_stack.interpolate(name);
    let raw = arg_stack.interpolate(value);
    let variant = parse_variant(&raw);

    let outcome = match GlobalsRepo::set(dp, &name, variant, false).await {
        Ok(()) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(e.to_string()),
    };

    let finished_at = OffsetDateTime::now_utc();
    let duration_ms = (finished_at - started_at).whole_milliseconds().max(0) as u64;

    SubActionTelemetry {
        index,
        kind: "SetGlobal".to_string(),
        started_at,
        duration_ms,
        outcome,
    }
}

fn parse_variant(s: &str) -> Variant {
    if let Ok(i) = s.parse::<i64>() {
        return Variant::Int(i);
    }
    if let Ok(f) = s.parse::<f64>()
        && let Ok(v) = Variant::float(f)
    {
        return v;
    }
    if s.eq_ignore_ascii_case("true") {
        return Variant::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Variant::Bool(false);
    }
    Variant::String(s.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_variant_integer() {
        assert!(matches!(parse_variant("42"), Variant::Int(42)));
    }

    #[test]
    fn parse_variant_negative_integer() {
        assert!(matches!(parse_variant("-7"), Variant::Int(-7)));
    }

    #[test]
    fn parse_variant_float() {
        let v = parse_variant("3.99");
        assert!(matches!(v, Variant::Float(f) if (f - 3.99).abs() < 1e-10));
    }

    #[test]
    fn parse_variant_bool_true() {
        assert!(matches!(parse_variant("true"), Variant::Bool(true)));
        assert!(matches!(parse_variant("TRUE"), Variant::Bool(true)));
    }

    #[test]
    fn parse_variant_bool_false() {
        assert!(matches!(parse_variant("false"), Variant::Bool(false)));
        assert!(matches!(parse_variant("False"), Variant::Bool(false)));
    }

    #[test]
    fn parse_variant_string_fallback() {
        assert!(matches!(parse_variant("hello"), Variant::String(s) if s == "hello"));
    }

    #[test]
    fn parse_variant_empty_string_falls_back_to_string() {
        assert!(matches!(parse_variant(""), Variant::String(s) if s.is_empty()));
    }
}
