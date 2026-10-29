use forge_types::ScriptId;
use serde::{Deserialize, Serialize};

use crate::TypedEvent;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptStarted {
    pub script_id: Option<ScriptId>,
    pub script_name: Option<String>,
    pub is_inline: bool,
}

impl TypedEvent for ScriptStarted {
    const KIND: &'static str = "script.exec";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptFailed {
    pub script_id: Option<ScriptId>,
    pub script_name: Option<String>,
    pub is_inline: bool,
    pub error_type: String,
    pub message: String,
}

impl TypedEvent for ScriptFailed {
    const KIND: &'static str = "script.error";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptLogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptLogged {
    pub level: ScriptLogLevel,
    pub message: String,
    pub script_id: Option<ScriptId>,
}

impl TypedEvent for ScriptLogged {
    const KIND: &'static str = "script.log";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptHttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptHttpCall {
    pub method: ScriptHttpMethod,
    pub url_normalized: String,
    pub status: u16,
    pub duration_ms: u64,
    pub truncated: bool,
}

impl TypedEvent for ScriptHttpCall {
    const KIND: &'static str = "script.http_call";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptReloaded {
    pub script_id: ScriptId,
    pub script_name: String,
}

impl TypedEvent for ScriptReloaded {
    const KIND: &'static str = "script.reloaded";
}
