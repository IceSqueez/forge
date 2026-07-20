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

pub(crate) fn extract_referenced_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            continue;
        }
        let mut token = String::new();
        let mut closed = false;
        for inner in chars.by_ref() {
            if inner == '%' {
                closed = true;
                break;
            }
            token.push(inner);
        }
        if closed {
            let name = token.trim();
            if !name.is_empty() {
                names.push(name.to_owned());
            }
        }
    }
    names
}

pub(super) fn sanitize_var_name(name: &str) -> String {
    let trimmed = name.trim();
    let unwrapped = trimmed
        .strip_prefix('%')
        .and_then(|inner| inner.strip_suffix('%'))
        .unwrap_or(trimmed);
    unwrapped.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_var_name_trims_then_strips_one_percent_pair_then_trims() {
        // Contract: trim -> peel exactly ONE enclosing %...% -> trim again.
        for (input, expected) in [
            ("index", "index"),
            (" index ", "index"),
            ("%index%", "index"),
            ("% index %", "index"),
            ("%%x%%", "%x%"), // only ONE pair peeled, inner pair survives
            ("", ""),
            ("%index", "%index"), // unbalanced (no trailing %) stays verbatim
            ("index%", "index%"), // unbalanced (no leading %) stays verbatim
        ] {
            assert_eq!(sanitize_var_name(input), expected, "input {input:?}");
        }
    }
}
