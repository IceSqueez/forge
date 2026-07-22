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

/// Distinct from [`VariantType`]: carries abbreviated caps labels ("INT", "STR") for UI pills, not the lowercase serialization tag.
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

    pub fn contract_name(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
            Self::String => "string",
            Self::Datetime => "datetime",
            Self::Array => "array",
            Self::Object => "object",
        }
    }

    /// Case-sensitive: `"INT"` returns `None`; only exact lowercase names match.
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

pub(crate) fn array_summary(items: &[Variant]) -> String {
    let uniform = items.first().map(VariantKind::from_variant).filter(|kind| {
        items
            .iter()
            .all(|item| VariantKind::from_variant(item) == *kind)
    });
    match uniform {
        Some(kind) => format!("{}[{}]", kind.contract_name(), items.len()),
        None => format!("[{}]", items.len()),
    }
}

/// Scalars render verbatim; arrays/objects collapse to a compact non-empty summary (`int[3]` / `object{2}`).
pub fn display_scalar(value: &Variant) -> String {
    match value {
        Variant::Array(items) => array_summary(items),
        Variant::Object(map) => format!("object{{{}}}", map.len()),
        scalar => scalar.to_string(),
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

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
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

    /// Depth ≤ 32, element count ≤ 10_000, null rejected, strings never auto-promoted.
    pub fn from_json(value: serde_json::Value) -> Result<Self, VariantError> {
        from_json_inner(value, 0)
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    pub fn to_plain_json(&self) -> serde_json::Value {
        match self {
            Variant::Int(n) => serde_json::Value::from(*n),
            Variant::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Variant::Bool(b) => serde_json::Value::Bool(*b),
            Variant::String(s) => serde_json::Value::String(s.clone()),
            Variant::Datetime(dt) => serde_json::Value::String(
                dt.format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| dt.to_string()),
            ),
            Variant::Array(items) => {
                serde_json::Value::Array(items.iter().map(Variant::to_plain_json).collect())
            }
            Variant::Object(map) => serde_json::Value::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), v.to_plain_json()))
                    .collect(),
            ),
        }
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
    fn serde_roundtrip_each_variant() {
        let mut obj_map = BTreeMap::new();
        obj_map.insert("x".to_owned(), Variant::Int(99));
        for v in [
            Variant::Int(42),
            Variant::float(1.25).unwrap(),
            Variant::Bool(false),
            Variant::String("hello".into()),
            Variant::Datetime(time::OffsetDateTime::UNIX_EPOCH),
            Variant::Array(vec![Variant::Int(1), Variant::Bool(true)]),
            Variant::Object(obj_map),
        ] {
            let json = serde_json::to_string(&v).unwrap();
            let back: Variant = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
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

    #[test]
    fn display_scalar_renders_scalars_verbatim_and_containers_as_summary() {
        let mut obj = BTreeMap::new();
        obj.insert("a".to_owned(), Variant::Int(1));
        obj.insert("b".to_owned(), Variant::Int(2));
        let cases: Vec<(Variant, &str)> = vec![
            (Variant::Int(42), "42"),
            (Variant::Int(-7), "-7"),
            (Variant::float(1.5).unwrap(), "1.5"),
            (Variant::Bool(true), "true"),
            (Variant::Bool(false), "false"),
            (Variant::String("hello world".into()), "hello world"),
            (
                Variant::Datetime(time::OffsetDateTime::UNIX_EPOCH),
                "1970-01-01T00:00:00Z",
            ),
            (
                Variant::Array(vec![Variant::Int(1), Variant::Int(2), Variant::Int(3)]),
                "int[3]",
            ),
            (
                Variant::Array(vec![Variant::Int(1), Variant::Bool(true)]),
                "[2]",
            ),
            (Variant::Array(vec![]), "[0]"),
            (Variant::Object(obj), "object{2}"),
            (Variant::Object(BTreeMap::new()), "object{0}"),
        ];
        for (value, expected) in cases {
            assert_eq!(display_scalar(&value), expected, "value {value:?}");
        }
    }
}
