use forge_types::SubActionConfig;

pub fn effective_config(default: &SubActionConfig, overrides: &SubActionConfig) -> SubActionConfig {
    let mut out = default.clone();
    for (key, value) in overrides {
        out.insert(key.clone(), value.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::Variant;

    use super::effective_config;

    fn config<const N: usize>(entries: [(&str, Variant); N]) -> BTreeMap<String, Variant> {
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    fn object<const N: usize>(entries: [(&str, Variant); N]) -> Variant {
        Variant::Object(config(entries))
    }

    #[test]
    fn overrides_win_key_by_key_over_the_defaults() {
        let cases = [
            (
                "default only",
                config([("a", Variant::Int(1))]),
                config([]),
                config([("a", Variant::Int(1))]),
            ),
            (
                "override only",
                config([]),
                config([("x", Variant::Int(42))]),
                config([("x", Variant::Int(42))]),
            ),
            (
                "shared key",
                config([("only_default", Variant::Int(1)), ("k", Variant::Int(2))]),
                config([("k", Variant::Int(99)), ("only_override", Variant::Int(3))]),
                config([
                    ("only_default", Variant::Int(1)),
                    ("k", Variant::Int(99)),
                    ("only_override", Variant::Int(3)),
                ]),
            ),
            (
                "nested object replaced whole",
                config([(
                    "nested",
                    object([("a", Variant::Int(1)), ("b", Variant::Int(2))]),
                )]),
                config([("nested", object([("c", Variant::Int(3))]))]),
                config([("nested", object([("c", Variant::Int(3))]))]),
            ),
        ];
        for (case, default, overrides, expected) in cases {
            assert_eq!(effective_config(&default, &overrides), expected, "{case}");
        }
    }
}
