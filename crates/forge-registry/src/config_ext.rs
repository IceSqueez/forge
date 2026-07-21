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
