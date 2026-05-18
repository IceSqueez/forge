use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

const MAX_COLLECTION_LEN: usize = 10_000;
const MAX_DEPTH: u8 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VariantType {
    Int,
    Float,
    Bool,
    String,
    Datetime,
    Array,
    Object,
}

impl fmt::Display for VariantType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
            Self::String => "string",
            Self::Datetime => "datetime",
            Self::Array => "array",
            Self::Object => "object",
        })
    }
}

impl std::str::FromStr for VariantType {
    type Err = VariantError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "int" => Ok(Self::Int),
            "float" => Ok(Self::Float),
            "bool" => Ok(Self::Bool),
            "string" => Ok(Self::String),
            "datetime" => Ok(Self::Datetime),
            "array" => Ok(Self::Array),
            "object" => Ok(Self::Object),
            _ => Err(VariantError::UnknownTypeTag(s.to_owned())),
        }
    }
}

/// Display-facing discriminant for a [`Variant`] value.
///
/// Distinct from [`VariantType`]: this type carries abbreviated caps labels
/// ("INT", "STR", …) suitable for UI pills and script contract annotations,
/// while [`VariantType`] owns the lowercase serialisation tag used in storage
/// and error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VariantKind {
    Int,
    Float,
    Bool,
    String,
    Datetime,
    Array,
    Object,
}

impl VariantKind {
    pub fn from_variant(v: &Variant) -> Self {
        match v {
            Variant::Int(_) => Self::Int,
            Variant::Float(_) => Self::Float,
            Variant::Bool(_) => Self::Bool,
            Variant::String(_) => Self::String,
            Variant::Datetime(_) => Self::Datetime,
            Variant::Array(_) => Self::Array,
            Variant::Object(_) => Self::Object,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Int => "INT",
            Self::Float => "FLOAT",
            Self::Bool => "BOOL",
            Self::String => "STR",
            Self::Datetime => "TIME",
            Self::Array => "ARR",
            Self::Object => "OBJ",
        }
    }

    /// Parses a lowercase type name from a script contract annotation.
    ///
    /// Returns `None` for unknown names and for names that are not all-lowercase
    /// (e.g. `"INT"` → `None`).
    pub fn from_contract_name(name: &str) -> Option<Self> {
        match name {
            "int" => Some(Self::Int),
            "float" => Some(Self::Float),
            "bool" => Some(Self::Bool),
            "string" => Some(Self::String),
            "datetime" => Some(Self::Datetime),
            "array" => Some(Self::Array),
            "object" => Some(Self::Object),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VariantError {
    #[error("type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        expected: VariantType,
        actual: VariantType,
    },

    #[error("invalid float: {reason}")]
    InvalidFloat { reason: &'static str },

    #[error("datetime parse failed: {0}")]
    DatetimeParse(#[from] time::error::Parse),

    #[error("JSON conversion failed: {0}")]
    JsonConversion(std::string::String),

    #[error("null is not a supported Variant value")]
    NullNotSupported,

    #[error("unknown type tag: {0}")]
    UnknownTypeTag(std::string::String),
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum Variant {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(std::string::String),
    #[serde(with = "time::serde::rfc3339")]
    Datetime(time::OffsetDateTime),
    Array(Vec<Variant>),
    Object(BTreeMap<std::string::String, Variant>),
}

impl fmt::Debug for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "Variant::Int({v})"),
            Self::Float(v) => write!(f, "Variant::Float({v})"),
            Self::Bool(v) => write!(f, "Variant::Bool({v})"),
            Self::String(v) => write!(f, "Variant::String({v:?})"),
            Self::Datetime(v) => write!(f, "Variant::Datetime({v})"),
            Self::Array(v) => write!(f, "Variant::Array([{} items])", v.len()),
            Self::Object(v) => write!(f, "Variant::Object({{{} keys}})", v.len()),
        }
    }
}

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::String(v) => write!(f, "{v}"),
            Self::Datetime(v) => {
                let s = v
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| v.to_string());
                write!(f, "{s}")
            }
            Self::Array(v) => write!(f, "[{} items]", v.len()),
            Self::Object(v) => write!(f, "{{{} keys}}", v.len()),
        }
    }
}

impl Variant {
    /// Rejects NaN and infinite values.
    pub fn float(f: f64) -> Result<Self, VariantError> {
        if f.is_nan() {
            return Err(VariantError::InvalidFloat { reason: "NaN" });
        }
        if f.is_infinite() {
            return Err(VariantError::InvalidFloat { reason: "infinite" });
        }
        Ok(Self::Float(f))
    }

    pub fn type_tag(&self) -> VariantType {
        match self {
            Self::Int(_) => VariantType::Int,
            Self::Float(_) => VariantType::Float,
            Self::Bool(_) => VariantType::Bool,
            Self::String(_) => VariantType::String,
            Self::Datetime(_) => VariantType::Datetime,
            Self::Array(_) => VariantType::Array,
            Self::Object(_) => VariantType::Object,
        }
    }

    pub fn is_int(&self) -> bool {
        matches!(self, Self::Int(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Self::Float(_))
    }

    pub fn is_bool(&self) -> bool {
        matches!(self, Self::Bool(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }

    pub fn is_datetime(&self) -> bool {
        matches!(self, Self::Datetime(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }

    pub fn as_int(&self) -> Option<i64> {
        if let Self::Int(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        if let Self::Float(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        if let Self::String(v) = self {
            Some(v.as_str())
        } else {
            None
        }
    }

    pub fn as_datetime(&self) -> Option<&time::OffsetDateTime> {
        if let Self::Datetime(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_array(&self) -> Option<&[Variant]> {
        if let Self::Array(v) = self {
            Some(v.as_slice())
        } else {
            None
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<std::string::String, Variant>> {
        if let Self::Object(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn into_int(self) -> Result<i64, VariantError> {
        match self {
            Self::Int(v) => Ok(v),
            other => Err(VariantError::TypeMismatch {
                expected: VariantType::Int,
                actual: other.type_tag(),
            }),
        }
    }

    pub fn into_float(self) -> Result<f64, VariantError> {
        match self {
            Self::Float(v) => Ok(v),
            other => Err(VariantError::TypeMismatch {
                expected: VariantType::Float,
                actual: other.type_tag(),
            }),
        }
    }

    pub fn into_bool(self) -> Result<bool, VariantError> {
        match self {
            Self::Bool(v) => Ok(v),
            other => Err(VariantError::TypeMismatch {
                expected: VariantType::Bool,
                actual: other.type_tag(),
            }),
        }
    }

    pub fn into_string(self) -> Result<std::string::String, VariantError> {
        match self {
            Self::String(v) => Ok(v),
            other => Err(VariantError::TypeMismatch {
                expected: VariantType::String,
                actual: other.type_tag(),
            }),
        }
    }

    pub fn into_datetime(self) -> Result<time::OffsetDateTime, VariantError> {
        match self {
            Self::Datetime(v) => Ok(v),
            other => Err(VariantError::TypeMismatch {
                expected: VariantType::Datetime,
                actual: other.type_tag(),
            }),
        }
    }

    pub fn into_array(self) -> Result<Vec<Variant>, VariantError> {
        match self {
            Self::Array(v) => Ok(v),
            other => Err(VariantError::TypeMismatch {
                expected: VariantType::Array,
                actual: other.type_tag(),
            }),
        }
    }

    pub fn into_object(self) -> Result<BTreeMap<std::string::String, Variant>, VariantError> {
        match self {
            Self::Object(v) => Ok(v),
            other => Err(VariantError::TypeMismatch {
                expected: VariantType::Object,
                actual: other.type_tag(),
            }),
        }
    }

    /// Depth ≤ 32, element count ≤ 10_000, null rejected, strings never auto-promoted.
    pub fn from_json(value: serde_json::Value) -> Result<Self, VariantError> {
        from_json_inner(value, 0)
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

fn from_json_inner(value: serde_json::Value, depth: u8) -> Result<Variant, VariantError> {
    if depth > MAX_DEPTH {
        return Err(VariantError::JsonConversion(
            "depth limit exceeded".to_owned(),
        ));
    }

    match value {
        serde_json::Value::Null => Err(VariantError::NullNotSupported),

        serde_json::Value::Bool(b) => Ok(Variant::Bool(b)),

        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Variant::Int(i))
            } else if let Some(f) = n.as_f64() {
                Variant::float(f)
            } else {
                Err(VariantError::JsonConversion(
                    "numeric value out of range".to_owned(),
                ))
            }
        }

        serde_json::Value::String(s) => Ok(Variant::String(s)),

        serde_json::Value::Array(arr) => {
            if arr.len() > MAX_COLLECTION_LEN {
                return Err(VariantError::JsonConversion(
                    "array element count limit exceeded".to_owned(),
                ));
            }
            let items = arr
                .into_iter()
                .map(|v| from_json_inner(v, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Variant::Array(items))
        }

        serde_json::Value::Object(map) => {
            if map.len() > MAX_COLLECTION_LEN {
                return Err(VariantError::JsonConversion(
                    "object key count limit exceeded".to_owned(),
                ));
            }

            let has_type_key = map.contains_key("type");
            let has_value_key = map.contains_key("value");

            if has_type_key && has_value_key && map.len() == 2 {
                let json_obj = serde_json::Value::Object(map);
                return serde_json::from_value::<Variant>(json_obj)
                    .map_err(|e| VariantError::JsonConversion(e.to_string()));
            }

            let entries = map
                .into_iter()
                .map(|(k, v)| from_json_inner(v, depth + 1).map(|vv| (k, vv)))
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            Ok(Variant::Object(entries))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn float_rejects_nan() {
        assert!(matches!(
            Variant::float(f64::NAN),
            Err(VariantError::InvalidFloat { reason: "NaN" })
        ));
    }

    #[test]
    fn float_rejects_positive_infinity() {
        assert!(matches!(
            Variant::float(f64::INFINITY),
            Err(VariantError::InvalidFloat { reason: "infinite" })
        ));
    }

    #[test]
    fn float_rejects_negative_infinity() {
        assert!(matches!(
            Variant::float(f64::NEG_INFINITY),
            Err(VariantError::InvalidFloat { reason: "infinite" })
        ));
    }

    #[test]
    fn float_accepts_finite_value() {
        let v = Variant::float(1.5).unwrap();
        assert_eq!(v.as_float(), Some(1.5));
    }

    #[test]
    fn type_tag_correct_for_each_variant() {
        assert_eq!(Variant::Int(1).type_tag(), VariantType::Int);
        assert_eq!(Variant::float(1.0).unwrap().type_tag(), VariantType::Float);
        assert_eq!(Variant::Bool(true).type_tag(), VariantType::Bool);
        assert_eq!(Variant::String("s".into()).type_tag(), VariantType::String);
        assert_eq!(
            Variant::Datetime(time::OffsetDateTime::UNIX_EPOCH).type_tag(),
            VariantType::Datetime
        );
        assert_eq!(Variant::Array(vec![]).type_tag(), VariantType::Array);
        assert_eq!(
            Variant::Object(BTreeMap::new()).type_tag(),
            VariantType::Object
        );
    }

    #[test]
    fn serde_roundtrip_int() {
        let v = Variant::Int(42);
        let json = serde_json::to_string(&v).unwrap();
        let back: Variant = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn serde_roundtrip_float() {
        let v = Variant::float(1.25).unwrap();
        let json = serde_json::to_string(&v).unwrap();
        let back: Variant = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn serde_roundtrip_bool() {
        let v = Variant::Bool(false);
        let json = serde_json::to_string(&v).unwrap();
        let back: Variant = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn serde_roundtrip_string() {
        let v = Variant::String("hello".into());
        let json = serde_json::to_string(&v).unwrap();
        let back: Variant = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn serde_roundtrip_datetime() {
        let v = Variant::Datetime(time::OffsetDateTime::UNIX_EPOCH);
        let json = serde_json::to_string(&v).unwrap();
        let back: Variant = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn serde_roundtrip_array() {
        let v = Variant::Array(vec![Variant::Int(1), Variant::Bool(true)]);
        let json = serde_json::to_string(&v).unwrap();
        let back: Variant = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn serde_roundtrip_object() {
        let mut map = BTreeMap::new();
        map.insert("x".to_owned(), Variant::Int(99));
        let v = Variant::Object(map);
        let json = serde_json::to_string(&v).unwrap();
        let back: Variant = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn serde_format_is_adjacently_tagged() {
        let v = Variant::Int(7);
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["type"], "int");
        assert_eq!(json["value"], 7);
    }

    #[test]
    fn from_json_null_returns_error() {
        assert!(matches!(
            Variant::from_json(json!(null)),
            Err(VariantError::NullNotSupported)
        ));
    }

    #[test]
    fn from_json_string_stays_string_not_promoted() {
        let v = Variant::from_json(json!("2026-05-16T00:00:00Z")).unwrap();
        assert!(
            v.is_string(),
            "RFC3339-looking strings must NOT be promoted to Datetime"
        );
    }

    #[test]
    fn from_json_roundtrip_via_to_json() {
        let original = Variant::Array(vec![
            Variant::Int(1),
            Variant::String("test".into()),
            Variant::Bool(false),
        ]);
        let json = original.to_json();
        let back = Variant::from_json(json).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn from_json_enforces_array_size_limit() {
        let big_array: Vec<serde_json::Value> =
            (0..=MAX_COLLECTION_LEN).map(|i| json!(i)).collect();
        let result = Variant::from_json(serde_json::Value::Array(big_array));
        assert!(
            matches!(result, Err(VariantError::JsonConversion(_))),
            "array exceeding limit must fail"
        );
    }

    #[test]
    fn from_json_enforces_object_size_limit() {
        let big_object: serde_json::Map<std::string::String, serde_json::Value> = (0
            ..=MAX_COLLECTION_LEN)
            .map(|i| (i.to_string(), json!(i)))
            .collect();
        let result = Variant::from_json(serde_json::Value::Object(big_object));
        assert!(
            matches!(result, Err(VariantError::JsonConversion(_))),
            "object exceeding limit must fail"
        );
    }

    #[test]
    fn from_json_enforces_depth_limit() {
        let mut nested: serde_json::Value = json!("leaf");
        for _ in 0..=MAX_DEPTH {
            nested = json!([nested]);
        }
        let result = Variant::from_json(nested);
        assert!(
            matches!(result, Err(VariantError::JsonConversion(_))),
            "depth exceeding limit must fail"
        );
    }

    #[test]
    fn into_wrong_type_returns_type_mismatch() {
        let result = Variant::Bool(true).into_int();
        assert!(matches!(
            result,
            Err(VariantError::TypeMismatch {
                expected: VariantType::Int,
                actual: VariantType::Bool,
            })
        ));
    }

    #[test]
    fn variant_type_display_values() {
        assert_eq!(VariantType::Int.to_string(), "int");
        assert_eq!(VariantType::Float.to_string(), "float");
        assert_eq!(VariantType::Bool.to_string(), "bool");
        assert_eq!(VariantType::String.to_string(), "string");
        assert_eq!(VariantType::Datetime.to_string(), "datetime");
        assert_eq!(VariantType::Array.to_string(), "array");
        assert_eq!(VariantType::Object.to_string(), "object");
    }

    #[test]
    fn variant_type_from_str_roundtrip() {
        for tag in &[
            "int", "float", "bool", "string", "datetime", "array", "object",
        ] {
            let vt: VariantType = tag.parse().unwrap();
            assert_eq!(vt.to_string(), *tag);
        }
    }

    #[test]
    fn variant_type_from_str_unknown_errors() {
        let result: Result<VariantType, _> = "binary".parse();
        assert!(matches!(result, Err(VariantError::UnknownTypeTag(_))));
    }

    #[test]
    fn variant_kind_from_variant_all_seven() {
        assert_eq!(
            VariantKind::from_variant(&Variant::Int(0)),
            VariantKind::Int
        );
        assert_eq!(
            VariantKind::from_variant(&Variant::float(1.0).unwrap()),
            VariantKind::Float
        );
        assert_eq!(
            VariantKind::from_variant(&Variant::Bool(true)),
            VariantKind::Bool
        );
        assert_eq!(
            VariantKind::from_variant(&Variant::String("hi".into())),
            VariantKind::String
        );
        assert_eq!(
            VariantKind::from_variant(&Variant::Datetime(time::OffsetDateTime::UNIX_EPOCH)),
            VariantKind::Datetime
        );
        assert_eq!(
            VariantKind::from_variant(&Variant::Array(vec![])),
            VariantKind::Array
        );
        assert_eq!(
            VariantKind::from_variant(&Variant::Object(BTreeMap::new())),
            VariantKind::Object
        );
    }

    #[test]
    fn variant_kind_labels_are_caps_abbreviations() {
        assert_eq!(VariantKind::Int.label(), "INT");
        assert_eq!(VariantKind::Float.label(), "FLOAT");
        assert_eq!(VariantKind::Bool.label(), "BOOL");
        assert_eq!(VariantKind::String.label(), "STR");
        assert_eq!(VariantKind::Datetime.label(), "TIME");
        assert_eq!(VariantKind::Array.label(), "ARR");
        assert_eq!(VariantKind::Object.label(), "OBJ");
    }

    #[test]
    fn variant_kind_from_contract_name_valid_lowercase() {
        assert_eq!(
            VariantKind::from_contract_name("int"),
            Some(VariantKind::Int)
        );
        assert_eq!(
            VariantKind::from_contract_name("float"),
            Some(VariantKind::Float)
        );
        assert_eq!(
            VariantKind::from_contract_name("bool"),
            Some(VariantKind::Bool)
        );
        assert_eq!(
            VariantKind::from_contract_name("string"),
            Some(VariantKind::String)
        );
        assert_eq!(
            VariantKind::from_contract_name("datetime"),
            Some(VariantKind::Datetime)
        );
        assert_eq!(
            VariantKind::from_contract_name("array"),
            Some(VariantKind::Array)
        );
        assert_eq!(
            VariantKind::from_contract_name("object"),
            Some(VariantKind::Object)
        );
    }

    #[test]
    fn variant_kind_from_contract_name_uppercase_rejected() {
        assert_eq!(VariantKind::from_contract_name("INT"), None);
        assert_eq!(VariantKind::from_contract_name("Float"), None);
        assert_eq!(VariantKind::from_contract_name("BOOL"), None);
    }

    #[test]
    fn variant_kind_from_contract_name_unknown_rejected() {
        assert_eq!(VariantKind::from_contract_name("binary"), None);
        assert_eq!(VariantKind::from_contract_name(""), None);
        assert_eq!(VariantKind::from_contract_name("number"), None);
    }

    #[test]
    fn display_format_non_allocating_for_collections() {
        let arr = Variant::Array(vec![Variant::Int(1), Variant::Int(2)]);
        assert_eq!(arr.to_string(), "[2 items]");

        let mut map = BTreeMap::new();
        map.insert("a".to_owned(), Variant::Int(1));
        let obj = Variant::Object(map);
        assert_eq!(obj.to_string(), "{1 keys}");
    }
}
