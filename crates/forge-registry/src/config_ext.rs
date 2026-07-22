use std::collections::BTreeMap;

use forge_types::{SubActionConfig, Variant};

use crate::error::RegistryError;

pub trait SubActionConfigExt {
    fn str(&self, key: &str) -> Option<&str>;
    fn str_nonempty(&self, key: &str) -> Option<&str>;
    fn int(&self, key: &str) -> Option<i64>;
    fn float(&self, key: &str) -> Option<f64>;
    fn bool(&self, key: &str) -> Option<bool>;
    fn array(&self, key: &str) -> Option<&[Variant]>;
    fn object(&self, key: &str) -> Option<&BTreeMap<String, Variant>>;

    fn require_str(&self, key: &str) -> Result<&str, RegistryError>;
    fn require_int(&self, key: &str) -> Result<i64, RegistryError>;
    fn require_float(&self, key: &str) -> Result<f64, RegistryError>;
    fn require_bool(&self, key: &str) -> Result<bool, RegistryError>;
    fn require_array(&self, key: &str) -> Result<&[Variant], RegistryError>;
    fn require_object(&self, key: &str) -> Result<&BTreeMap<String, Variant>, RegistryError>;
}

fn missing(key: &str) -> RegistryError {
    RegistryError::InvalidConfig(format!("field '{key}' is required"))
}

impl SubActionConfigExt for SubActionConfig {
    fn str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.as_str())
    }

    fn str_nonempty(&self, key: &str) -> Option<&str> {
        self.str(key).filter(|s| !s.is_empty())
    }

    fn int(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| v.as_int())
    }

    fn float(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(|v| v.as_float())
    }

    fn bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| v.as_bool())
    }

    fn array(&self, key: &str) -> Option<&[Variant]> {
        self.get(key).and_then(|v| v.as_array())
    }

    fn object(&self, key: &str) -> Option<&BTreeMap<String, Variant>> {
        self.get(key).and_then(|v| v.as_object())
    }

    fn require_str(&self, key: &str) -> Result<&str, RegistryError> {
        self.str_nonempty(key).ok_or_else(|| missing(key))
    }

    fn require_int(&self, key: &str) -> Result<i64, RegistryError> {
        self.int(key).ok_or_else(|| missing(key))
    }

    fn require_float(&self, key: &str) -> Result<f64, RegistryError> {
        self.float(key).ok_or_else(|| missing(key))
    }

    fn require_bool(&self, key: &str) -> Result<bool, RegistryError> {
        self.bool(key).ok_or_else(|| missing(key))
    }

    fn require_array(&self, key: &str) -> Result<&[Variant], RegistryError> {
        self.array(key).ok_or_else(|| missing(key))
    }

    fn require_object(&self, key: &str) -> Result<&BTreeMap<String, Variant>, RegistryError> {
        self.object(key).ok_or_else(|| missing(key))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_types::Variant;

    fn cfg(value: Variant) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("k".to_owned(), value);
        c
    }

    #[test]
    fn numeric_getters_do_not_cross_coerce_between_int_and_float() {
        assert_eq!(cfg(Variant::Int(5)).int("k"), Some(5));
        assert_eq!(cfg(Variant::Float(5.0)).int("k"), None);
        assert_eq!(cfg(Variant::Float(2.5)).float("k"), Some(2.5));
        assert_eq!(cfg(Variant::Int(2)).float("k"), None);
    }

    #[test]
    fn optional_getters_return_none_for_absent_key() {
        let empty = SubActionConfig::new();
        assert_eq!(empty.str("k"), None);
        assert_eq!(empty.str_nonempty("k"), None);
        assert_eq!(empty.int("k"), None);
        assert_eq!(empty.float("k"), None);
        assert_eq!(empty.bool("k"), None);
        assert_eq!(empty.array("k"), None);
        assert_eq!(empty.object("k"), None);
    }

    #[test]
    fn str_nonempty_rejects_empty_string_but_keeps_whitespace_only() {
        assert_eq!(cfg(Variant::String(String::new())).str("k"), Some(""));
        assert_eq!(cfg(Variant::String(String::new())).str_nonempty("k"), None);
        assert_eq!(
            cfg(Variant::String("   ".to_owned())).str_nonempty("k"),
            Some("   ")
        );
    }

    #[test]
    fn require_getters_name_the_missing_field_in_the_error() {
        let empty = SubActionConfig::new();
        let errors = [
            empty.require_str("k").err(),
            empty.require_int("k").err(),
            empty.require_float("k").err(),
            empty.require_bool("k").err(),
            empty.require_array("k").err(),
            empty.require_object("k").err(),
        ];
        for e in errors {
            assert_eq!(e.unwrap().to_string(), "field 'k' is required");
        }
    }

    #[test]
    fn require_str_treats_a_present_empty_string_as_missing() {
        assert_eq!(
            cfg(Variant::String(String::new()))
                .require_str("k")
                .unwrap_err()
                .to_string(),
            "field 'k' is required"
        );
        assert_eq!(
            cfg(Variant::String("  ".to_owned()))
                .require_str("k")
                .unwrap(),
            "  "
        );
    }

    #[test]
    fn require_getters_return_the_value_when_present_and_well_typed() {
        assert_eq!(
            cfg(Variant::String("v".to_owned()))
                .require_str("k")
                .unwrap(),
            "v"
        );
        assert_eq!(cfg(Variant::Int(7)).require_int("k").unwrap(), 7);
        assert_eq!(cfg(Variant::Float(1.5)).require_float("k").unwrap(), 1.5);
        assert!(cfg(Variant::Bool(true)).require_bool("k").unwrap());
        assert_eq!(
            cfg(Variant::Array(vec![Variant::Int(1)]))
                .require_array("k")
                .unwrap()
                .len(),
            1
        );
        assert!(
            cfg(Variant::Object(BTreeMap::new()))
                .require_object("k")
                .unwrap()
                .is_empty()
        );
    }
}
