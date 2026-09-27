use forge_types::Variant;

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
