use std::path::PathBuf;

use async_trait::async_trait;
use forge_types::{ClipId, OutputDevice};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredClip {
    pub id: ClipId,
    pub name: String,
    pub file_path: PathBuf,
    pub volume: f32,
    pub output_device: OutputDevice,
    pub hotkey: Option<String>,
    pub created_at: OffsetDateTime,
    pub category: String,
    pub loop_playback: bool,
    pub duration_secs: Option<f32>,
    pub builtin_id: Option<String>,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait SoundboardClipsRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<StoredClip>, StorageError>;
    async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, StorageError>;
    async fn save(&self, clip: &StoredClip) -> Result<(), StorageError>;
    /// Returns true if a row was removed.
    async fn delete(&self, id: ClipId) -> Result<bool, StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn SoundboardClipsRepo) {}
}
