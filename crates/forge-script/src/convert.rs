use std::collections::BTreeMap;

use forge_types::{VARIANT_MAX_COLLECTION_LEN, VARIANT_MAX_DEPTH, Variant, datetime_to_text};
use time::OffsetDateTime;

const DATETIME_TYPE_NAME: &str = "datetime";

pub(crate) fn register_variant_types(engine: &mut rhai::Engine) {
    engine.register_type_with_name::<OffsetDateTime>(DATETIME_TYPE_NAME);
    engine.register_fn("to_string", |dt: &mut OffsetDateTime| datetime_to_text(dt));
    engine.register_fn("to_debug", |dt: &mut OffsetDateTime| datetime_to_text(dt));
    engine.register_fn("==", |a: OffsetDateTime, b: OffsetDateTime| a == b);
    engine.register_fn("!=", |a: OffsetDateTime, b: OffsetDateTime| a != b);
    engine.register_fn("<", |a: OffsetDateTime, b: OffsetDateTime| a < b);
    engine.register_fn("<=", |a: OffsetDateTime, b: OffsetDateTime| a <= b);
    engine.register_fn(">", |a: OffsetDateTime, b: OffsetDateTime| a > b);
    engine.register_fn(">=", |a: OffsetDateTime, b: OffsetDateTime| a >= b);
}

pub(crate) fn variant_to_dynamic(v: Variant) -> rhai::Dynamic {
    match v {
        Variant::Int(i) => rhai::Dynamic::from(i),
        Variant::Float(f) => rhai::Dynamic::from(f),
        Variant::Bool(b) => rhai::Dynamic::from(b),
        Variant::String(s) => rhai::Dynamic::from(s),
        Variant::Datetime(dt) => rhai::Dynamic::from(dt),
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

pub(crate) fn dynamic_to_variant(d: rhai::Dynamic) -> Result<Variant, String> {
    dynamic_to_variant_at(d, 0)
}

fn dynamic_to_variant_at(d: rhai::Dynamic, depth: u8) -> Result<Variant, String> {
    if depth > VARIANT_MAX_DEPTH {
        return Err(format!(
            "value nested deeper than {VARIANT_MAX_DEPTH} levels cannot be stored"
        ));
    }
    let d = d.flatten();
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
    if d.is::<char>() {
        return Ok(Variant::String(d.cast::<char>().to_string()));
    }
    if d.is::<OffsetDateTime>() {
        return Ok(Variant::Datetime(d.cast::<OffsetDateTime>()));
    }
    if d.is::<rhai::Array>() {
        return array_to_variant(d.cast::<rhai::Array>(), depth);
    }
    if d.is::<rhai::Map>() {
        return map_to_variant(d.cast::<rhai::Map>(), depth);
    }
    Err(format!(
        "unsupported rhai type for Variant conversion: {}",
        d.type_name()
    ))
}

fn array_to_variant(items: rhai::Array, depth: u8) -> Result<Variant, String> {
    if items.len() > VARIANT_MAX_COLLECTION_LEN {
        return Err(format!(
            "array with more than {VARIANT_MAX_COLLECTION_LEN} items cannot be stored"
        ));
    }
    items
        .into_iter()
        .map(|item| dynamic_to_variant_at(item, depth + 1))
        .collect::<Result<Vec<_>, _>>()
        .map(Variant::Array)
}

fn map_to_variant(entries: rhai::Map, depth: u8) -> Result<Variant, String> {
    if entries.len() > VARIANT_MAX_COLLECTION_LEN {
        return Err(format!(
            "map with more than {VARIANT_MAX_COLLECTION_LEN} keys cannot be stored"
        ));
    }
    entries
        .into_iter()
        .map(|(key, value)| dynamic_to_variant_at(value, depth + 1).map(|v| (key.to_string(), v)))
        .collect::<Result<BTreeMap<_, _>, _>>()
        .map(Variant::Object)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::engine::{Engine, EngineConfig};
    use forge_types::Variant;
    use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

    fn at(year: i32, nanos: u32, offset: (i8, i8, i8)) -> OffsetDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(year, Month::March, 5).unwrap(),
            Time::from_hms_nano(14, 7, 9, nanos).unwrap(),
        )
        .assume_offset(UtcOffset::from_hms(offset.0, offset.1, offset.2).unwrap())
    }

    fn engine() -> Engine {
        Engine::with_config(EngineConfig {
            wall_time_ms: 30_000,
            ..EngineConfig::default()
        })
    }

    fn eval_with(
        bindings: &[(&str, Variant)],
        body: &str,
    ) -> Result<Option<Variant>, crate::ScriptError> {
        let mut scope = rhai::Scope::new();
        for (name, value) in bindings {
            scope.push_dynamic(*name, variant_to_dynamic(value.clone()));
        }
        engine().eval_script_with_scope_as_variant(body, &mut scope)
    }

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn object(entries: &[(&str, Variant)]) -> Variant {
        Variant::Object(
            entries
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        )
    }

    fn nested_arrays(levels: u8) -> rhai::Dynamic {
        let mut value = rhai::Dynamic::from(rhai::ImmutableString::from("leaf"));
        for _ in 0..levels {
            value = rhai::Dynamic::from(vec![value]);
        }
        value
    }

    fn array_of(len: usize) -> rhai::Dynamic {
        let items: rhai::Array = (0..len as i64).map(rhai::Dynamic::from).collect();
        rhai::Dynamic::from(items)
    }

    fn map_of(len: usize) -> rhai::Dynamic {
        let entries: rhai::Map = (0..len as i64)
            .map(|i| (i.to_string().into(), rhai::Dynamic::from(i)))
            .collect();
        rhai::Dynamic::from(entries)
    }

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

        let dt = at(2026, 0, (0, 0, 0));
        let dt_d = variant_to_dynamic(Variant::Datetime(dt));
        assert_eq!(dt_d.cast::<OffsetDateTime>(), dt);

        let arr_d = variant_to_dynamic(Variant::Array(vec![Variant::Int(1), Variant::Int(2)]));
        let arr = arr_d.cast::<rhai::Array>();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].clone().cast::<i64>(), 1);

        let obj_d = variant_to_dynamic(object(&[("key", Variant::Int(99))]));
        let map = obj_d.cast::<rhai::Map>();
        assert_eq!(map.get("key").unwrap().clone().cast::<i64>(), 99);
    }

    #[test]
    fn every_variant_type_survives_a_trip_through_a_script_unchanged() {
        let dt = at(2026, 500_000_000, (-5, -30, 0));
        for value in [
            Variant::Int(-7),
            Variant::Float(2.5),
            Variant::Bool(false),
            text(""),
            text("привіт"),
            Variant::Datetime(dt),
            Variant::Array(vec![]),
            object(&[]),
            Variant::Array(vec![
                Variant::Int(1),
                Variant::Datetime(dt),
                Variant::Array(vec![Variant::Array(vec![]), text("deep")]),
                object(&[("k", Variant::Bool(true))]),
            ]),
            object(&[
                ("when", Variant::Datetime(dt)),
                (
                    "list",
                    Variant::Array(vec![Variant::Float(0.5), object(&[])]),
                ),
                ("inner", object(&[("n", Variant::Int(3))])),
            ]),
        ] {
            let back = eval_with(&[("value", value.clone())], "let copy = value; copy").unwrap();
            assert_eq!(back, Some(value.clone()), "{value:?}");
        }
    }

    #[test]
    fn values_a_script_builds_convert_to_the_matching_variant() {
        for (body, expected) in [
            ("[]", Variant::Array(vec![])),
            ("#{}", object(&[])),
            ("'x'", text("x")),
            (
                r#"[1, 2.5, true, "s", 'c', [[]]]"#,
                Variant::Array(vec![
                    Variant::Int(1),
                    Variant::Float(2.5),
                    Variant::Bool(true),
                    text("s"),
                    text("c"),
                    Variant::Array(vec![Variant::Array(vec![])]),
                ]),
            ),
            (
                r#"#{name: "ana", tags: ["a"], meta: #{level: 2}}"#,
                object(&[
                    ("name", text("ana")),
                    ("tags", Variant::Array(vec![text("a")])),
                    ("meta", object(&[("level", Variant::Int(2))])),
                ]),
            ),
        ] {
            assert_eq!(eval_with(&[], body).unwrap(), Some(expected), "{body}");
        }
    }

    #[test]
    fn a_datetime_renders_in_a_script_as_the_same_text_every_other_renderer_uses() {
        for (value, expected) in [
            (at(2026, 0, (0, 0, 0)), "2026-03-05T14:07:09Z"),
            (at(2026, 0, (-5, -30, 0)), "2026-03-05T14:07:09-05:30"),
            (at(2026, 500_000_000, (0, 0, 0)), "2026-03-05T14:07:09.5Z"),
            (at(-1, 0, (0, 0, 0)), "-0001-03-05 14:07:09.0 +00:00:00"),
        ] {
            for body in ["value.to_string()", "`${value}`"] {
                let rendered = eval_with(&[("value", Variant::Datetime(value))], body).unwrap();
                assert_eq!(rendered, Some(text(expected)), "{body} for {value:?}");
            }
        }
    }

    #[test]
    fn a_datetime_reports_its_script_type_as_datetime() {
        let dt = Variant::Datetime(at(2026, 0, (0, 0, 0)));

        assert_eq!(
            eval_with(&[("value", dt)], "type_of(value)").unwrap(),
            Some(text("datetime"))
        );
    }

    #[test]
    fn datetimes_compare_by_instant_in_scripts() {
        let earlier = Variant::Datetime(at(2026, 0, (0, 0, 0)));
        let later = Variant::Datetime(at(2026, 1, (0, 0, 0)));
        let earlier_elsewhere = Variant::Datetime(
            at(2026, 0, (0, 0, 0)).to_offset(UtcOffset::from_hms(3, 0, 0).unwrap()),
        );
        let bindings = [
            ("a", earlier),
            ("b", later),
            ("a_shifted", earlier_elsewhere),
        ];
        for (body, expected) in [
            ("a < b", true),
            ("b < a", false),
            ("a <= b", true),
            ("a <= a", true),
            ("b <= a", false),
            ("b > a", true),
            ("a > b", false),
            ("b >= a", true),
            ("a >= a", true),
            ("a >= b", false),
            ("a == a_shifted", true),
            ("a == b", false),
            ("a != b", true),
            ("a != a_shifted", false),
        ] {
            assert_eq!(
                eval_with(&bindings, body).unwrap(),
                Some(Variant::Bool(expected)),
                "{body}"
            );
        }
    }

    #[test]
    fn dynamic_to_variant_accepts_values_sitting_exactly_on_the_depth_and_size_limits() {
        for (label, value) in [
            ("depth", nested_arrays(VARIANT_MAX_DEPTH)),
            ("array length", array_of(VARIANT_MAX_COLLECTION_LEN)),
            ("map keys", map_of(VARIANT_MAX_COLLECTION_LEN)),
        ] {
            let converted = dynamic_to_variant(value);
            assert!(converted.is_ok(), "{label}: {converted:?}");
        }
    }

    #[test]
    fn dynamic_to_variant_rejects_values_it_cannot_store() {
        for (label, value) in [
            ("unit", rhai::Dynamic::UNIT),
            ("NaN", rhai::Dynamic::from(f64::NAN)),
            (
                "unit inside an array",
                rhai::Dynamic::from(vec![rhai::Dynamic::from(1_i64), rhai::Dynamic::UNIT]),
            ),
            ("depth", nested_arrays(VARIANT_MAX_DEPTH + 1)),
            ("array length", array_of(VARIANT_MAX_COLLECTION_LEN + 1)),
            ("map keys", map_of(VARIANT_MAX_COLLECTION_LEN + 1)),
        ] {
            let converted = dynamic_to_variant(value);
            assert!(converted.is_err(), "{label}: {converted:?}");
        }
    }
}
