use std::collections::BTreeMap;

use forge_types::Variant;

/// Shallow merge of a patch onto a default config.
///
/// For each key in `overrides`, the patch value wins outright. Keys absent
/// from `overrides` fall through to their value in `default`. Nested
/// `Variant::Object` / `Variant::Array` values are NOT deep-merged — the
/// override replaces the whole sub-tree (consistent with RFC-047 §3).
pub fn effective_config(
    default: &BTreeMap<String, Variant>,
    overrides: &BTreeMap<String, Variant>,
) -> BTreeMap<String, Variant> {
    let mut out = default.clone();
    for (key, value) in overrides {
        out.insert(key.clone(), value.clone());
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::Variant;

    use super::effective_config;

    fn s(v: &str) -> Variant {
        Variant::String(v.to_owned())
    }

    #[test]
    fn empty_overrides_returns_default_clone() {
        let mut default = BTreeMap::new();
        default.insert("a".to_owned(), s("hello"));
        let result = effective_config(&default, &BTreeMap::new());
        assert_eq!(result, default);
    }

    #[test]
    fn key_absent_in_default_is_added() {
        let default = BTreeMap::new();
        let mut overrides = BTreeMap::new();
        overrides.insert("x".to_owned(), Variant::Int(42));
        let result = effective_config(&default, &overrides);
        assert_eq!(result.get("x"), Some(&Variant::Int(42)));
    }

    #[test]
    fn override_value_wins_for_shared_key() {
        let mut default = BTreeMap::new();
        default.insert("k".to_owned(), Variant::Int(1));
        let mut overrides = BTreeMap::new();
        overrides.insert("k".to_owned(), Variant::Int(99));
        let result = effective_config(&default, &overrides);
        assert_eq!(result.get("k"), Some(&Variant::Int(99)));
    }

    #[test]
    fn object_override_replaces_whole_subtree() {
        let mut default_obj = BTreeMap::new();
        default_obj.insert("a".to_owned(), Variant::Int(1));
        default_obj.insert("b".to_owned(), Variant::Int(2));

        let mut override_obj = BTreeMap::new();
        override_obj.insert("c".to_owned(), Variant::Int(3));

        let mut default = BTreeMap::new();
        default.insert("nested".to_owned(), Variant::Object(default_obj));

        let mut overrides = BTreeMap::new();
        overrides.insert("nested".to_owned(), Variant::Object(override_obj.clone()));

        let result = effective_config(&default, &overrides);

        let got = result
            .get("nested")
            .and_then(|v| {
                if let Variant::Object(m) = v {
                    Some(m.clone())
                } else {
                    None
                }
            })
            .expect("expected Variant::Object at key 'nested'");
        assert!(!got.contains_key("a"));
        assert!(!got.contains_key("b"));
        assert_eq!(got.get("c"), Some(&Variant::Int(3)));
    }

    #[test]
    fn empty_default_non_empty_overrides_equals_overrides() {
        let mut overrides = BTreeMap::new();
        overrides.insert("m".to_owned(), Variant::Bool(true));
        let result = effective_config(&BTreeMap::new(), &overrides);
        assert_eq!(result, overrides);
    }

    #[test]
    fn both_empty_produces_empty_map() {
        let result = effective_config(&BTreeMap::new(), &BTreeMap::new());
        assert!(result.is_empty());
    }

    #[test]
    fn result_contains_union_of_all_keys() {
        let mut default = BTreeMap::new();
        default.insert("only_default".to_owned(), Variant::Int(1));
        default.insert("shared".to_owned(), Variant::Int(2));

        let mut overrides = BTreeMap::new();
        overrides.insert("shared".to_owned(), Variant::Int(20));
        overrides.insert("only_override".to_owned(), Variant::Int(3));

        let result = effective_config(&default, &overrides);

        assert!(result.contains_key("only_default"));
        assert!(result.contains_key("shared"));
        assert!(result.contains_key("only_override"));
        assert_eq!(result.len(), 3);
    }
}
