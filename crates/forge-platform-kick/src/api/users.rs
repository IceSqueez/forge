use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_net::{Gateway, GatewayRequest};
use forge_platform_core::{AvatarLookup, PlatformError, ViewerAvatarSource};
use serde::Deserialize;

use crate::auth::credentials_manager::KickCredentialsManager;
use crate::gateway::{RefusalReasons, authorized, malformed, platform_error};

const USERS_PATH: &str = "/users";
const ID_PARAM: &str = "id";
const USERS_PER_REQUEST: usize = 50;

const USERS_REFUSALS: RefusalReasons = RefusalReasons {
    unauthorized: "users token rejected (401)",
    forbidden: Some("users forbidden (403); check user:read scope"),
};

#[derive(Deserialize)]
struct UsersEnvelope {
    #[serde(default)]
    data: Vec<UserData>,
}

#[derive(Deserialize)]
struct UserData {
    #[serde(default)]
    user_id: Option<u64>,
    #[serde(default)]
    profile_picture: Option<String>,
}

pub struct KickViewerAvatars {
    api: Gateway,
    credentials: Arc<KickCredentialsManager>,
}

impl KickViewerAvatars {
    pub fn new(api: Gateway, credentials: Arc<KickCredentialsManager>) -> Self {
        Self { api, credentials }
    }

    async fn profile_pictures(
        &self,
        user_ids: &[u64],
    ) -> Result<HashMap<String, String>, PlatformError> {
        let token = self.credentials.get_valid_access_token().await?;
        let request = user_ids.iter().fold(
            authorized(GatewayRequest::get(USERS_PATH), &token, USERS_PATH),
            |request, id| request.query(ID_PARAM, id.to_string()),
        );
        let response = self
            .api
            .send(request)
            .await
            .map_err(|failure| platform_error(failure, USERS_REFUSALS))?;
        let envelope: UsersEnvelope = response.json().map_err(malformed)?;
        Ok(envelope
            .data
            .into_iter()
            .filter_map(|user| {
                let picture = user.profile_picture.filter(|url| !url.is_empty())?;
                Some((user.user_id?.to_string(), picture))
            })
            .collect())
    }
}

#[async_trait]
impl ViewerAvatarSource for KickViewerAvatars {
    fn batch_limit(&self) -> usize {
        USERS_PER_REQUEST
    }

    async fn avatars(&self, viewer_ids: &[String]) -> AvatarLookup {
        let user_ids: Vec<u64> = viewer_ids
            .iter()
            .filter_map(|id| id.trim().parse().ok())
            .take(USERS_PER_REQUEST)
            .collect();
        if user_ids.is_empty() {
            return AvatarLookup::Answered(HashMap::new());
        }
        match self.profile_pictures(&user_ids).await {
            Ok(pictures) => AvatarLookup::Answered(pictures),
            Err(error) => {
                tracing::debug!(%error, "kick viewer avatars unavailable");
                AvatarLookup::Unavailable
            }
        }
    }
}
