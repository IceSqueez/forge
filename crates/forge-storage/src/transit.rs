use forge_types::{ActionId, ScriptId, TriggerInstanceId, Variant};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use time::OffsetDateTime;

pub const CURRENT_FORMAT_VERSION: u32 = 1;

pub const BUNDLE_FORMAT_VERSION: u32 = 1;

/// Oldest bundle version the importer will accept without a hard error.
/// Bumped only when a structural field removal or type change would produce
/// incorrect data if parsed as an older version (not on field addition).
pub const MINIMUM_SUPPORTED_BUNDLE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalTransit {
    pub name: String,
    pub value: Variant,
    pub persisted: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub last_modified: OffsetDateTime,
    pub reads: u64,
    pub writes: u64,
}

/// `format_version` lets future importers route to the correct parser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalsExport {
    pub format_version: u32,
    pub globals: Vec<GlobalTransit>,
}

impl GlobalsExport {
    pub fn new(globals: Vec<GlobalTransit>) -> Self {
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            globals,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionTransit {
    pub id: ActionId,
    pub name: String,
    pub group: Option<String>,
    pub enabled: bool,
    pub concurrent: bool,
    pub bypass_pause: bool,
    pub execution_mode: String,
    pub description: Option<String>,
    // Why: sub-actions are stored as a JSON blob in SQLite; re-parsing the polymorphic
    // chain here would require transit types for every SubAction variant — deferred to
    // the runtime crate that owns the type hierarchy.
    pub sub_actions: JsonValue,
    pub created_at: String,
    pub last_modified: String,
}

/// Only user-defined instances are exported; default instances are recreated from
/// platform crate registrations on the target install.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerInstanceTransit {
    pub id: TriggerInstanceId,
    pub kind_id: String,
    pub name: String,
    pub enabled: bool,
    pub overrides: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptTransit {
    pub id: ScriptId,
    /// Identity key for conflict detection on import (case-sensitive).
    pub name: String,
    pub body: String,
    pub enabled: bool,
    pub contract: JsonValue,
    pub body_hash: String,
    pub created_at: String,
    pub last_modified: String,
}

/// Only `format_version` is required; absence of an entity array equals an empty
/// array. Unknown fields are silently ignored for forward compat (no
/// `deny_unknown_fields`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleDocument {
    pub format_version: u32,
    /// ISO 8601 creation timestamp, informational only.
    pub created_at: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub actions: Vec<ActionTransit>,
    #[serde(default)]
    pub trigger_instances: Vec<TriggerInstanceTransit>,
    #[serde(default)]
    pub scripts: Vec<ScriptTransit>,
    #[serde(default)]
    pub globals: Vec<GlobalTransit>,
}

impl BundleDocument {
    pub fn new() -> Self {
        Self {
            format_version: BUNDLE_FORMAT_VERSION,
            created_at: None,
            display_name: None,
            description: None,
            actions: Vec::new(),
            trigger_instances: Vec::new(),
            scripts: Vec::new(),
            globals: Vec::new(),
        }
    }
}

impl Default for BundleDocument {
    fn default() -> Self {
        Self::new()
    }
}
