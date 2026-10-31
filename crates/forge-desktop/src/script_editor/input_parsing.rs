use forge_components::tr;
use forge_types::{Variant, VariantKind};

pub(super) const ARRAY_INPUT_EXAMPLE: &str = "[12, 13, 14]";
pub(super) const OBJECT_INPUT_EXAMPLE: &str = r#"{"key": "value"}"#;
pub(super) const DATETIME_INPUT_EXAMPLE: &str = "2026-01-31T18:04:00Z";

pub(super) fn input_syntax_example(kind: VariantKind) -> Option<&'static str> {
    match kind {
        VariantKind::Array => Some(ARRAY_INPUT_EXAMPLE),
        VariantKind::Object => Some(OBJECT_INPUT_EXAMPLE),
        VariantKind::Datetime => Some(DATETIME_INPUT_EXAMPLE),
        VariantKind::Int | VariantKind::Float | VariantKind::Bool | VariantKind::String => None,
    }
}

#[derive(Debug)]
pub(super) struct ScriptInputRejection {
    pub(super) name: String,
    pub(super) reason: ScriptInputReason,
}

#[derive(Debug)]
pub(super) enum ScriptInputReason {
    NotInteger,
    NotFloat,
    NotBool,
    NotDatetime,
    WrongShape {
        shape: String,
        example: &'static str,
    },
    Invalid(String),
}

impl ScriptInputRejection {
    pub(super) fn new(name: &str, reason: ScriptInputReason) -> Self {
        Self {
            name: name.to_owned(),
            reason,
        }
    }

    pub(super) fn user_message(&self) -> String {
        let name = self.name.as_str();
        match &self.reason {
            ScriptInputReason::NotInteger => tr!("script_input_must_be_integer", name = name),
            ScriptInputReason::NotFloat => tr!("script_input_must_be_float", name = name),
            ScriptInputReason::NotBool => tr!("script_input_must_be_bool", name = name),
            ScriptInputReason::NotDatetime => tr!(
                "script_input_must_be_datetime",
                name = name,
                example = DATETIME_INPUT_EXAMPLE
            ),
            ScriptInputReason::WrongShape { shape, example } => tr!(
                "script_input_must_be_shape",
                name = name,
                shape = shape.as_str(),
                example = *example
            ),
            ScriptInputReason::Invalid(reason) => tr!(
                "script_input_invalid",
                name = name,
                reason = reason.as_str()
            ),
        }
    }
}

fn parse_json_input(
    name: &str,
    raw: &str,
    expected: VariantKind,
    shape: String,
    example: &'static str,
) -> Result<Variant, ScriptInputRejection> {
    let wrong_shape = |shape: String| {
        ScriptInputRejection::new(name, ScriptInputReason::WrongShape { shape, example })
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw.trim()) else {
        return Err(wrong_shape(shape));
    };
    let variant = Variant::from_plain_json(value).map_err(|e| invalid_input(name, &e))?;
    if VariantKind::from_variant(&variant) == expected {
        Ok(variant)
    } else {
        Err(wrong_shape(shape))
    }
}

fn invalid_input(name: &str, reason: &dyn std::fmt::Display) -> ScriptInputRejection {
    ScriptInputRejection::new(name, ScriptInputReason::Invalid(reason.to_string()))
}

pub(super) fn parse_input_to_variant(
    name: &str,
    kind: VariantKind,
    raw: &str,
) -> Result<Variant, ScriptInputRejection> {
    let rejected = |reason| ScriptInputRejection::new(name, reason);
    match kind {
        VariantKind::Int => raw
            .trim()
            .parse::<i64>()
            .map(Variant::Int)
            .map_err(|_| rejected(ScriptInputReason::NotInteger)),
        VariantKind::Float => raw
            .trim()
            .parse::<f64>()
            .map_err(|_| rejected(ScriptInputReason::NotFloat))
            .and_then(|f| Variant::float(f).map_err(|e| invalid_input(name, &e))),
        VariantKind::Bool => match raw.trim() {
            "true" => Ok(Variant::Bool(true)),
            "false" => Ok(Variant::Bool(false)),
            _ => Err(rejected(ScriptInputReason::NotBool)),
        },
        VariantKind::String => Ok(Variant::String(raw.to_owned())),
        VariantKind::Array => parse_json_input(
            name,
            raw,
            VariantKind::Array,
            tr!("script_input_shape_array"),
            ARRAY_INPUT_EXAMPLE,
        ),
        VariantKind::Object => parse_json_input(
            name,
            raw,
            VariantKind::Object,
            tr!("script_input_shape_object"),
            OBJECT_INPUT_EXAMPLE,
        ),
        VariantKind::Datetime => forge_types::datetime_from_text(raw)
            .map(Variant::Datetime)
            .ok_or_else(|| rejected(ScriptInputReason::NotDatetime)),
    }
}
