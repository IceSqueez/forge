use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::{AvatarLookup, ViewerAvatarSource};

use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::helix_envelope::rows;

const USERS_PATH: &str = "/helix/users";
const ID_PARAM: &str = "id";
const ID_KEY: &str = "id";
const PROFILE_IMAGE_KEY: &str = "profile_image_url";
const USERS_PER_REQUEST: usize = 100;

fn users_request(viewer_ids: &[String]) -> HelixRequest {
    viewer_ids.iter().fold(
        HelixRequest::new(HelixMethod::Get, USERS_PATH),
        |request, id| request.query(ID_PARAM, id.as_str()),
    )
}

fn profile_images(response: &serde_json::Value) -> HashMap<String, String> {
    rows(response)
        .filter_map(|user| {
            let id = user[ID_KEY].as_str()?;
            let image = user[PROFILE_IMAGE_KEY].as_str()?;
            (!id.is_empty() && !image.is_empty()).then(|| (id.to_owned(), image.to_owned()))
        })
        .collect()
}

pub(crate) struct TwitchViewerAvatars {
    helix: Arc<dyn HelixTransport>,
}

impl TwitchViewerAvatars {
    pub(crate) fn new(helix: Arc<dyn HelixTransport>) -> Self {
        Self { helix }
    }
}

#[async_trait]
impl ViewerAvatarSource for TwitchViewerAvatars {
    fn batch_limit(&self) -> usize {
        USERS_PER_REQUEST
    }

    async fn avatars(&self, viewer_ids: &[String]) -> AvatarLookup {
        let asked: Vec<String> = viewer_ids
            .iter()
            .filter(|id| !id.is_empty())
            .take(USERS_PER_REQUEST)
            .cloned()
            .collect();
        if asked.is_empty() {
            return AvatarLookup::Answered(HashMap::new());
        }
        match self.helix.execute(users_request(&asked)).await {
            Ok(response) => AvatarLookup::Answered(profile_images(&response)),
            Err(error) => {
                tracing::debug!(%error, "twitch viewer avatars unavailable");
                AvatarLookup::Unavailable
            }
        }
    }
}
