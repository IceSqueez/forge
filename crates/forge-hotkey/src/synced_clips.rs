use std::sync::Arc;

use async_trait::async_trait;
use forge_storage::{SoundboardClipsRepo, StorageError, StoredClip};
use forge_types::ClipId;

use crate::clip_bindings::canonical_clip;
use crate::reconciler::HotkeyReconciler;

pub struct HotkeySyncedClipsRepo {
    inner: Arc<dyn SoundboardClipsRepo>,
    reconciler: Option<Arc<HotkeyReconciler>>,
}

impl HotkeySyncedClipsRepo {
    pub fn wrap(
        inner: Arc<dyn SoundboardClipsRepo>,
        reconciler: Option<Arc<HotkeyReconciler>>,
    ) -> Arc<dyn SoundboardClipsRepo> {
        Arc::new(Self { inner, reconciler })
    }

    async fn sync(&self) {
        if let Some(reconciler) = &self.reconciler {
            reconciler.reconcile().await;
        }
    }
}

#[async_trait]
impl SoundboardClipsRepo for HotkeySyncedClipsRepo {
    async fn list(&self) -> Result<Vec<StoredClip>, StorageError> {
        self.inner.list().await
    }

    async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, StorageError> {
        self.inner.get(id).await
    }

    async fn save(&self, clip: &StoredClip) -> Result<(), StorageError> {
        let canonical = canonical_clip(clip)?;
        self.inner.save(&canonical).await?;
        self.sync().await;
        Ok(())
    }

    async fn delete(&self, id: ClipId) -> Result<bool, StorageError> {
        let deleted = self.inner.delete(id).await?;
        self.sync().await;
        Ok(deleted)
    }
}
