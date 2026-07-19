use forge_types::Variant;

/// Parses an interpolated string into the most specific `Variant` type that matches.
///
/// Integers win over floats; case-insensitive "true"/"false" yield `Bool`.
/// Falls through to `Variant::String` when no numeric or boolean parse succeeds.
pub(super) fn parse_variant(s: &str) -> Variant {
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
