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

/// Does not support rhai `Array`/`Map`; callers needing collection round-trips handle those branches explicitly.
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
    fn variant_to_dynamic_maps_each_variant_to_expected_rhai_type() {
        let int_d = variant_to_dynamic(Variant::Int(42));
        assert_eq!(int_d.cast::<i64>(), 42);

        let float_d = variant_to_dynamic(Variant::Float(2.5));
        assert!((float_d.cast::<f64>() - 2.5).abs() < f64::EPSILON);

        let bool_d = variant_to_dynamic(Variant::Bool(true));
        assert!(bool_d.cast::<bool>());

        let str_d = variant_to_dynamic(Variant::String("hello".to_owned()));
        assert_eq!(str_d.cast::<rhai::ImmutableString>().as_str(), "hello");

        let dt = OffsetDateTime::from_unix_timestamp(0).unwrap();
        let dt_d = variant_to_dynamic(Variant::Datetime(dt));
        assert!(dt_d.cast::<rhai::ImmutableString>().contains("1970"));

        let arr_d = variant_to_dynamic(Variant::Array(vec![Variant::Int(1), Variant::Int(2)]));
        let arr = arr_d.cast::<rhai::Array>();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].clone().cast::<i64>(), 1);

        let mut obj = std::collections::BTreeMap::new();
        obj.insert("key".to_owned(), Variant::Int(99));
        let obj_d = variant_to_dynamic(Variant::Object(obj));
        let map = obj_d.cast::<rhai::Map>();
        assert_eq!(map.get("key").unwrap().clone().cast::<i64>(), 99);
    }

    #[test]
    fn dynamic_to_variant_roundtrips_primitives_and_string() {
        assert_eq!(
            dynamic_to_variant(rhai::Dynamic::from(7i64)).unwrap(),
            Variant::Int(7)
        );
        assert_eq!(
            dynamic_to_variant(rhai::Dynamic::from(2.5f64)).unwrap(),
            Variant::Float(2.5)
        );
        assert_eq!(
            dynamic_to_variant(rhai::Dynamic::from(true)).unwrap(),
            Variant::Bool(true)
        );
        assert_eq!(
            dynamic_to_variant(rhai::Dynamic::from(rhai::ImmutableString::from("hi"))).unwrap(),
            Variant::String("hi".to_owned())
        );
    }

    #[test]
    fn dynamic_to_variant_rejects_unsupported_kinds() {
        let arr: rhai::Array = vec![rhai::Dynamic::from(1i64)];
        assert!(dynamic_to_variant(rhai::Dynamic::from(arr)).is_err());
        assert!(dynamic_to_variant(rhai::Dynamic::UNIT).is_err());
    }
}
