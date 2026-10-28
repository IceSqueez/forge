use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::bundle::manifest::{BundleKind, ContentEntry, check_head, check_schema, expect_kind};
use crate::documents::archive::DocumentManifest;
use crate::error::TransferError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupManifest {
    pub format_version: u32,
    pub kind: BundleKind,
    pub forge_version: String,
    pub schema_version: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub contents: Vec<ContentEntry>,
}

impl DocumentManifest for SetupManifest {
    fn contents(&self) -> &[ContentEntry] {
        &self.contents
    }
}

pub(crate) fn parse_setup(bytes: &[u8]) -> Result<SetupManifest, TransferError> {
    expect_kind(check_head(bytes)?, BundleKind::Setup)?;
    let manifest: SetupManifest = serde_json::from_slice(bytes).map_err(TransferError::damaged)?;
    check_schema(manifest.schema_version)?;
    Ok(manifest)
}
