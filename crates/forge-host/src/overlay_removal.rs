use forge_runtime::{OverlayServiceHandle, RuntimeError};
use forge_storage::{OverlayId, SettingsRepo};

use crate::audio_router::AudioRouter;
use crate::audio_routes::{release_receiver, restore_receiver};

#[derive(Debug)]
pub enum OverlayRemoval {
    Missing,
    Removed { folder_left: Option<RuntimeError> },
}

pub async fn remove_overlay(
    service: &OverlayServiceHandle,
    settings: &dyn SettingsRepo,
    router: &AudioRouter,
    id: &OverlayId,
) -> Result<OverlayRemoval, RuntimeError> {
    let was_receiver = release_receiver(settings, id).await?;
    let removed = service.delete(id).await;
    if !matches!(removed, Ok(true)) {
        if was_receiver {
            restore_receiver(settings, id).await;
        }
        return removed.map(|_| OverlayRemoval::Missing);
    }
    if was_receiver {
        router.apply().await;
    }
    Ok(OverlayRemoval::Removed {
        folder_left: service.discard_assets(id).await.err(),
    })
}
