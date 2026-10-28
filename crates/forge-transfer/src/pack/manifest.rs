use forge_types::IntegrationId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::bundle::manifest::{BundleKind, ContentEntry, check_head, check_schema, expect_kind};
use crate::documents::reference::PackReference;
use crate::error::TransferError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackManifest {
    pub format_version: u32,
    pub kind: BundleKind,
    pub forge_version: String,
    pub schema_version: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub name: String,
    pub author: String,
    pub description: String,
    pub required_integrations: Vec<IntegrationId>,
    pub external_references: Vec<PackReference>,
    pub unresolved_references: Vec<PackReference>,
    pub unknown_kinds: Vec<String>,
    pub contents: Vec<ContentEntry>,
}

pub(crate) fn parse_pack(bytes: &[u8]) -> Result<PackManifest, TransferError> {
    expect_kind(check_head(bytes)?, BundleKind::Pack)?;
    let manifest: PackManifest = serde_json::from_slice(bytes).map_err(TransferError::damaged)?;
    check_schema(manifest.schema_version)?;
    Ok(manifest)
}
