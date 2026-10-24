use forge_types::{Variant, display_scalar};

const LIST_SEPARATOR: &str = "\n";

pub(crate) fn editable_text(value: &Variant) -> String {
    match value {
        Variant::Array(items) if items.iter().all(is_scalar) => items
            .iter()
            .map(display_scalar)
            .collect::<Vec<_>>()
            .join(LIST_SEPARATOR),
        Variant::Array(_) | Variant::Object(_) => {
            let json = value.to_plain_json();
            serde_json::to_string_pretty(&json).unwrap_or_else(|_| json.to_string())
        }
        scalar => scalar.to_string(),
    }
}

pub(crate) fn editable_line(value: &Variant) -> String {
    if is_scalar(value) {
        value.to_string()
    } else {
        value.to_plain_json().to_string()
    }
}

fn is_scalar(value: &Variant) -> bool {
    !matches!(value, Variant::Array(_) | Variant::Object(_))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn text(s: &str) -> Variant {
        Variant::String(s.to_owned())
    }

    fn object() -> Variant {
        Variant::Object(BTreeMap::from([
            ("a".to_owned(), Variant::Int(1)),
            (
                "list".to_owned(),
                Variant::Array(vec![text("x"), Variant::Bool(true)]),
            ),
        ]))
    }

    #[test]
    fn editable_text_renders_each_shape_as_editable_source() {
        for (value, expected) in [
            (
                Variant::Array(vec![text("one"), Variant::Int(2), Variant::Float(2.5)]),
                "one\n2\n2.5",
            ),
            (Variant::Array(vec![]), ""),
            (
                Variant::Array(vec![Variant::Array(vec![Variant::Int(1)]), text("b")]),
                "[\n  [\n    1\n  ],\n  \"b\"\n]",
            ),
            (
                object(),
                "{\n  \"a\": 1,\n  \"list\": [\n    \"x\",\n    true\n  ]\n}",
            ),
            (Variant::Object(BTreeMap::new()), "{}"),
            (text("line one\nline two"), "line one\nline two"),
            (Variant::Int(-7), "-7"),
            (Variant::Bool(false), "false"),
        ] {
            assert_eq!(editable_text(&value), expected, "{value:?}");
        }
    }

    #[test]
    fn editable_line_renders_containers_as_compact_json() {
        for (value, expected) in [
            (object(), r#"{"a":1,"list":["x",true]}"#),
            (Variant::Object(BTreeMap::new()), "{}"),
            (
                Variant::Array(vec![text("a"), Variant::Int(1)]),
                r#"["a",1]"#,
            ),
            (text("plain"), "plain"),
            (Variant::Float(0.25), "0.25"),
        ] {
            assert_eq!(editable_line(&value), expected, "{value:?}");
        }
    }

    #[test]
    fn container_text_parses_back_to_the_same_value() {
        for value in [
            object(),
            Variant::Array(vec![Variant::Array(vec![Variant::Int(1)]), text("b")]),
        ] {
            for rendered in [editable_text(&value), editable_line(&value)] {
                let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();
                assert_eq!(Variant::from_plain_json(json).unwrap(), value, "{rendered}");
            }
        }
    }
}
