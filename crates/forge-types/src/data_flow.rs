use serde::{Deserialize, Serialize};

use crate::variant::VariantKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VariableSchema {
    pub variables: Vec<DeclaredVariable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredVariable {
    pub name: String,
    pub kind: VariantKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthesis: Option<SynthesisHint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SynthesisHint {
    Username,
    DisplayName,
    Message,
    BoundedInt { min: i64, max: i64 },
}
