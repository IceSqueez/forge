use forge_types::{ActionId, ScriptId, TriggerInstanceId, Variant};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use time::OffsetDateTime;

pub const CURRENT_FORMAT_VERSION: u32 = 1;

pub const BUNDLE_FORMAT_VERSION: u32 = 1;

/// Bump only for a structural field removal or type change that would misparse an
/// older bundle; field additions alone don't require a bump.
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
    // Kept as raw JSON: typing this would need a transit type per SubAction variant,
    // owned by the runtime crate, not this one.
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

/// Unknown fields are silently ignored (no `deny_unknown_fields`) for forward compat.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    /// Skips on identity match; never modifies or deletes an existing entity.
    MergeAdd,
    /// Wipes Actions/user-defined TriggerInstances/Scripts/persisted Globals before
    /// inserting; credentials, settings, user_globals, and event_log survive. Calling
    /// this method IS the confirmation - the UI owns the guard.
    ReplaceConfirm,
}

/// Carries both display names so the UI can offer a per-item override without re-querying storage.
#[derive(Debug, Clone)]
pub struct SkippedEntity {
    pub bundle_display_name: String,
    pub local_display_name: String,
}

/// Hard failures (malformed JSON, version too old, DB write error) are `Err`; everything
/// else lands here. A `format_version` newer than current is a warning, not an error.
#[derive(Debug, Clone, Default)]
pub struct BundleImportOutcome {
    pub actions_inserted: u32,
    pub trigger_instances_inserted: u32,
    pub scripts_inserted: u32,
    pub globals_inserted: u32,
    /// Always empty outside `MergeAdd`; wipes leave nothing to collide with.
    pub actions_skipped: Vec<SkippedEntity>,
    pub trigger_instances_skipped: Vec<SkippedEntity>,
    pub scripts_skipped: Vec<SkippedEntity>,
    pub globals_skipped: Vec<SkippedEntity>,
    pub warnings: Vec<String>,
}

/// `document` is always populated even when `warnings` is non-empty.
#[derive(Debug, Clone)]
pub struct BundleExportOutcome {
    pub document: BundleDocument,
    pub warnings: Vec<String>,
}
