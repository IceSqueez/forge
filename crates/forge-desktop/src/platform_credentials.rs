use std::sync::Arc;

use forge_platform_core::{PlatformError, credential_storage_error};
use forge_storage::{CredentialId, CredentialsRepo};
use forge_types::PlatformId;

#[derive(Clone, Default)]
pub struct PlatformCredentialManagers {
    pub twitch: Option<Arc<forge_platform_twitch::TwitchCredentialsManager>>,
    pub youtube: Option<Arc<forge_platform_youtube::YoutubeCredentialsManager>>,
    pub kick: Option<Arc<forge_platform_kick::KickCredentialsManager>>,
}

impl PlatformCredentialManagers {
    pub async fn forget(
        &self,
        platform: PlatformId,
        unmanaged: &dyn CredentialsRepo,
    ) -> Result<(), PlatformError> {
        match platform {
            PlatformId::Twitch => match &self.twitch {
                Some(manager) => manager.clear().await,
                None => delete_unmanaged(unmanaged, platform).await,
            },
            PlatformId::YouTube => match &self.youtube {
                Some(manager) => manager.clear().await,
                None => delete_unmanaged(unmanaged, platform).await,
            },
            PlatformId::Kick => match &self.kick {
                Some(manager) => manager.clear().await,
                None => delete_unmanaged(unmanaged, platform).await,
            },
        }
    }
}

async fn delete_unmanaged(
    repo: &dyn CredentialsRepo,
    platform: PlatformId,
) -> Result<(), PlatformError> {
    repo.delete(&CredentialId::new(credential_key(platform)))
        .await
        .map(drop)
        .map_err(|e| credential_storage_error(platform.as_str(), e))
}

fn credential_key(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::Twitch => forge_platform_twitch::TWITCH_CREDENTIAL_ID,
        PlatformId::YouTube => forge_platform_youtube::CREDENTIAL_KEY,
        PlatformId::Kick => forge_platform_kick::CREDENTIAL_KEY,
    }
}
