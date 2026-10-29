use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::TypedEvent;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalSet {
    pub key: String,
    pub new_value: Value,
    pub prev_value: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persisted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

impl TypedEvent for GlobalSet {
    const KIND: &'static str = "global.set";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalIncremented {
    pub key: String,
    pub delta: i64,
    pub new_value: Value,
}

impl TypedEvent for GlobalIncremented {
    const KIND: &'static str = "global.incremented";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalDecremented {
    pub key: String,
    pub delta: i64,
    pub new_value: Value,
}

impl TypedEvent for GlobalDecremented {
    const KIND: &'static str = "global.decremented";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalToggled {
    pub key: String,
    pub new_value: bool,
    pub prev_value: bool,
}

impl TypedEvent for GlobalToggled {
    const KIND: &'static str = "global.toggled";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalArrayAppended {
    pub key: String,
    pub new_length: usize,
    pub element: Value,
}

impl TypedEvent for GlobalArrayAppended {
    const KIND: &'static str = "global.array_appended";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalArrayRemoved {
    pub key: String,
    pub new_length: usize,
    pub element: Value,
}

impl TypedEvent for GlobalArrayRemoved {
    const KIND: &'static str = "global.array_removed";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalDeleted {
    pub key: String,
}

impl TypedEvent for GlobalDeleted {
    const KIND: &'static str = "global.deleted";
}
