use forge_types::Variant;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const CURRENT_FORMAT_VERSION: u32 = 1;

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

/// Top-level envelope for a globals export file. `format_version` allows a future
/// importer to route to the correct parser without structural guessing.
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
