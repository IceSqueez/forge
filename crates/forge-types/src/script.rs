use serde::{Deserialize, Serialize};

use crate::variant::VariantKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ScriptContract {
    pub inputs: Vec<ScriptInput>,
    pub returns: Option<VariantKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptInput {
    pub name: String,
    pub kind: VariantKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationDiagnostic {
    /// 0-indexed line number in the source.
    pub line: usize,
    pub message: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn script_contract_serde_roundtrip_with_inputs_and_return() {
        let c = ScriptContract {
            inputs: vec![
                ScriptInput {
                    name: "user".to_owned(),
                    kind: VariantKind::String,
                },
                ScriptInput {
                    name: "count".to_owned(),
                    kind: VariantKind::Int,
                },
            ],
            returns: Some(VariantKind::Bool),
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: ScriptContract = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn script_input_json_shape_is_object_not_tuple() {
        let input = ScriptInput {
            name: "user".to_owned(),
            kind: VariantKind::String,
        };
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["name"], "user");
        assert_eq!(json["kind"], "string");
        assert!(json.is_object());
    }

    #[test]
    fn annotation_diagnostic_serde_roundtrip() {
        let d = AnnotationDiagnostic {
            line: 7,
            message: "type mismatch".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: AnnotationDiagnostic = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn script_contract_inputs_array_in_json() {
        let c = ScriptContract {
            inputs: vec![ScriptInput {
                name: "x".to_owned(),
                kind: VariantKind::Int,
            }],
            returns: None,
        };
        let json = serde_json::to_value(&c).unwrap();
        assert!(json["inputs"].is_array());
        assert_eq!(json["inputs"][0]["name"], "x");
        assert_eq!(json["inputs"][0]["kind"], "int");
    }
}
