use std::collections::BTreeMap;

use async_trait::async_trait;
use forge_types::Variant;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;

pub type OverlayConfig = BTreeMap<String, Variant>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OverlayId(String);

impl OverlayId {
    pub fn new(slug: impl Into<String>) -> Self {
        Self(slug.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for OverlayId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct OverlayCredential(String);

impl OverlayCredential {
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for OverlayCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayDefinition {
    pub id: OverlayId,
    pub display_name: String,
    pub kind_id: String,
    pub enabled: bool,
    pub position: i64,
    pub config: OverlayConfig,
    pub config_schema_version: u32,
    pub generator_version: u32,
    pub source_overrides: Vec<String>,
    pub credential: OverlayCredential,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait OverlayRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<OverlayDefinition>, StorageError>;

    async fn get(&self, id: &OverlayId) -> Result<Option<OverlayDefinition>, StorageError>;

    async fn get_by_credential(
        &self,
        credential: &OverlayCredential,
    ) -> Result<Option<OverlayDefinition>, StorageError>;

    /// Mints a stable identity slug from `display_name` (suffixed on collision) and a
    /// fresh read-scoped credential; the returned identity never changes afterward.
    async fn create(
        &self,
        display_name: &str,
        kind_id: &str,
        config_schema_version: u32,
    ) -> Result<OverlayDefinition, StorageError>;

    /// Updates every field except identity on an EXISTING row; an unknown id
    /// yields [`StorageError::NotFound`] - rows are only ever created by
    /// [`Self::create`], which mints the immutable identity.
    async fn save(&self, definition: &OverlayDefinition) -> Result<(), StorageError>;

    /// Returns true if a row was found and flipped.
    async fn set_enabled(&self, id: &OverlayId, enabled: bool) -> Result<bool, StorageError>;

    /// Returns true if a row was removed.
    async fn delete(&self, id: &OverlayId) -> Result<bool, StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "b8f1c07a4e2d9a6b5c4d3e2f1a0b9c8d";

    fn definition() -> OverlayDefinition {
        OverlayDefinition {
            id: OverlayId::new("alert-box"),
            display_name: "Alert Box".to_owned(),
            kind_id: "forge.chat".to_owned(),
            enabled: true,
            position: 0,
            config: OverlayConfig::new(),
            config_schema_version: 1,
            generator_version: 0,
            source_overrides: Vec::new(),
            credential: OverlayCredential::new(TOKEN),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn debug_output_never_carries_the_overlay_credential() {
        let credential = OverlayCredential::new(TOKEN);

        assert!(
            !format!("{credential:?}").contains(TOKEN),
            "credential leaked from its own Debug"
        );
        assert!(
            !format!("{:?}", definition()).contains(TOKEN),
            "credential leaked through the definition Debug"
        );
    }
}
