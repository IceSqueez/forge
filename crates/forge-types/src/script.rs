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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn script_contract_persisted_json_shape_names_every_field() {
        let c = ScriptContract {
            inputs: vec![ScriptInput {
                name: "user".to_owned(),
                kind: VariantKind::String,
            }],
            returns: Some(VariantKind::Int),
        };
        let json = serde_json::to_value(&c).unwrap();
        assert!(json["inputs"].is_array());
        assert!(json["inputs"][0].is_object());
        assert_eq!(json["inputs"][0]["name"], "user");
        assert_eq!(json["inputs"][0]["kind"], "string");
        assert_eq!(json["returns"], "int");
    }

    #[test]
    fn script_contract_survives_a_persistence_round_trip_with_and_without_a_return() {
        for returns in [None, Some(VariantKind::Bool)] {
            let c = ScriptContract {
                inputs: vec![ScriptInput {
                    name: "count".to_owned(),
                    kind: VariantKind::Int,
                }],
                returns,
            };
            let json = serde_json::to_string(&c).unwrap();
            let back: ScriptContract = serde_json::from_str(&json).unwrap();
            assert_eq!(c, back);
        }
    }
}
