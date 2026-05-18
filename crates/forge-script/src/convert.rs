use forge_types::Variant;
use time::format_description::well_known::Rfc3339;

pub(crate) fn variant_to_dynamic(v: Variant) -> rhai::Dynamic {
    match v {
        Variant::Int(i) => rhai::Dynamic::from(i),
        Variant::Float(f) => rhai::Dynamic::from(f),
        Variant::Bool(b) => rhai::Dynamic::from(b),
        Variant::String(s) => rhai::Dynamic::from(s),
        Variant::Datetime(dt) => rhai::Dynamic::from(dt.format(&Rfc3339).unwrap_or_default()),
        Variant::Array(arr) => {
            let a: rhai::Array = arr.into_iter().map(variant_to_dynamic).collect();
            rhai::Dynamic::from(a)
        }
        Variant::Object(obj) => {
            let m: rhai::Map = obj
                .into_iter()
                .map(|(k, v)| (k.into(), variant_to_dynamic(v)))
                .collect();
            rhai::Dynamic::from(m)
        }
    }
}

/// Converts a [`rhai::Dynamic`] to a [`Variant`].
///
/// Supports `Int`, `Float`, `Bool`, and `String`. Rhai `Array` and `Map` are
/// not supported — callers that need collection round-trips must handle
/// those branches explicitly. Returns `Err` for unrecognised types.
pub(crate) fn dynamic_to_variant(d: rhai::Dynamic) -> Result<Variant, String> {
    if d.is::<i64>() {
        return Ok(Variant::Int(d.cast::<i64>()));
    }
    if d.is::<f64>() {
        return Variant::float(d.cast::<f64>()).map_err(|e| e.to_string());
    }
    if d.is::<bool>() {
        return Ok(Variant::Bool(d.cast::<bool>()));
    }
    if d.is::<rhai::ImmutableString>() {
        return Ok(Variant::String(
            d.cast::<rhai::ImmutableString>().to_string(),
        ));
    }
    Err(format!(
        "unsupported rhai type for Variant conversion: {}",
        d.type_name()
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_types::Variant;
    use time::OffsetDateTime;

    #[test]
    fn variant_to_dynamic_int() {
        let d = variant_to_dynamic(Variant::Int(42));
        assert!(d.is::<i64>());
        assert_eq!(d.cast::<i64>(), 42);
    }

    #[test]
    fn variant_to_dynamic_float() {
        let d = variant_to_dynamic(Variant::Float(2.5));
        assert!(d.is::<f64>());
        assert!((d.cast::<f64>() - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn variant_to_dynamic_bool_true() {
        let d = variant_to_dynamic(Variant::Bool(true));
        assert!(d.is::<bool>());
        assert!(d.cast::<bool>());
    }

    #[test]
    fn variant_to_dynamic_bool_false() {
        let d = variant_to_dynamic(Variant::Bool(false));
        assert!(d.is::<bool>());
        assert!(!d.cast::<bool>());
    }

    #[test]
    fn variant_to_dynamic_string() {
        let d = variant_to_dynamic(Variant::String("hello".to_owned()));
        assert!(d.is::<rhai::ImmutableString>());
        assert_eq!(d.cast::<rhai::ImmutableString>().as_str(), "hello");
    }

    #[test]
    fn variant_to_dynamic_datetime_becomes_iso_string() {
        let dt = OffsetDateTime::from_unix_timestamp(0).unwrap();
        let d = variant_to_dynamic(Variant::Datetime(dt));
        assert!(d.is::<rhai::ImmutableString>());
        let s = d.cast::<rhai::ImmutableString>();
        assert!(s.contains("1970"), "ISO string must contain year: {s}");
    }

    #[test]
    fn variant_to_dynamic_array_nested() {
        let arr = vec![Variant::Int(1), Variant::Int(2)];
        let d = variant_to_dynamic(Variant::Array(arr));
        assert!(d.is::<rhai::Array>());
        let a = d.cast::<rhai::Array>();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].clone().cast::<i64>(), 1);
    }

    #[test]
    fn variant_to_dynamic_object() {
        let mut obj = std::collections::BTreeMap::new();
        obj.insert("key".to_owned(), Variant::Int(99));
        let d = variant_to_dynamic(Variant::Object(obj));
        assert!(d.is::<rhai::Map>());
        let m = d.cast::<rhai::Map>();
        let v = m.get("key").unwrap().clone().cast::<i64>();
        assert_eq!(v, 99);
    }

    #[test]
    fn dynamic_to_variant_int() {
        let result = dynamic_to_variant(rhai::Dynamic::from(7i64)).unwrap();
        assert_eq!(result, Variant::Int(7));
    }

    #[test]
    fn dynamic_to_variant_float() {
        let result = dynamic_to_variant(rhai::Dynamic::from(2.5f64)).unwrap();
        assert_eq!(result, Variant::Float(2.5));
    }

    #[test]
    fn dynamic_to_variant_bool() {
        let result = dynamic_to_variant(rhai::Dynamic::from(true)).unwrap();
        assert_eq!(result, Variant::Bool(true));
    }

    #[test]
    fn dynamic_to_variant_string() {
        let result =
            dynamic_to_variant(rhai::Dynamic::from(rhai::ImmutableString::from("hi"))).unwrap();
        assert_eq!(result, Variant::String("hi".to_owned()));
    }

    #[test]
    fn dynamic_to_variant_array_returns_err() {
        let arr: rhai::Array = vec![rhai::Dynamic::from(1i64)];
        let result = dynamic_to_variant(rhai::Dynamic::from(arr));
        assert!(result.is_err());
    }

    #[test]
    fn dynamic_to_variant_unit_returns_err() {
        let result = dynamic_to_variant(rhai::Dynamic::UNIT);
        assert!(result.is_err());
    }
}
