use async_trait::async_trait;

pub use forge_voice::{AliasId, AssignmentStrategy, IgnoreProfile, VoiceAlias};

use crate::StorageError;

#[async_trait]
pub trait VoiceAliasRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<VoiceAlias>, StorageError>;
    async fn upsert(&self, alias: &VoiceAlias) -> Result<(), StorageError>;
    async fn delete(&self, id: &AliasId) -> Result<(), StorageError>;
    async fn find_by_viewer(&self, viewer_id: &str) -> Result<Option<VoiceAlias>, StorageError>;
    async fn get_strategy(&self) -> Result<AssignmentStrategy, StorageError>;
    async fn set_strategy(&self, strategy: &AssignmentStrategy) -> Result<(), StorageError>;
    async fn get_ignore_profile(&self) -> Result<IgnoreProfile, StorageError>;
    async fn set_ignore_profile(&self, profile: &IgnoreProfile) -> Result<(), StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _dyn_safe(_: &dyn VoiceAliasRepo) {}
}
