use std::collections::HashMap;
use std::sync::Arc;

use forge_types::BadgeImage;

use crate::error::TwitchError;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::helix_envelope::rows;

const GLOBAL_BADGES_PATH: &str = "/helix/chat/badges/global";
const CHANNEL_BADGES_PATH: &str = "/helix/chat/badges";
const BROADCASTER_PARAM: &str = "broadcaster_id";
const SET_ID_KEY: &str = "set_id";
const VERSION_ID_KEY: &str = "id";
const VERSIONS_KEY: &str = "versions";
const TITLE_KEY: &str = "title";
const IMAGE_KEY: &str = "image_url_2x";

type BadgeKey = (String, String);

#[derive(Default)]
pub(crate) struct BadgeCatalog {
    channel: HashMap<BadgeKey, BadgeImage>,
    global: HashMap<BadgeKey, BadgeImage>,
}

fn catalog_entries(response: &serde_json::Value) -> HashMap<BadgeKey, BadgeImage> {
    rows(response)
        .flat_map(|set| {
            let set_id = set[SET_ID_KEY].as_str().unwrap_or_default();
            set[VERSIONS_KEY]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(move |version| {
                    let version_id = version[VERSION_ID_KEY].as_str()?;
                    let url = version[IMAGE_KEY].as_str()?;
                    if set_id.is_empty() || url.is_empty() {
                        return None;
                    }
                    let title = version[TITLE_KEY].as_str().unwrap_or(set_id);
                    Some((
                        (set_id.to_owned(), version_id.to_owned()),
                        BadgeImage {
                            title: title.to_owned(),
                            url: url.to_owned(),
                        },
                    ))
                })
        })
        .collect()
}

impl BadgeCatalog {
    pub(crate) async fn fetch(
        helix: &Arc<dyn HelixTransport>,
        broadcaster_id: &str,
    ) -> Result<Self, TwitchError> {
        let global = helix
            .execute(HelixRequest::new(HelixMethod::Get, GLOBAL_BADGES_PATH))
            .await?;
        let channel = helix
            .execute(
                HelixRequest::new(HelixMethod::Get, CHANNEL_BADGES_PATH)
                    .query(BROADCASTER_PARAM, broadcaster_id),
            )
            .await?;
        Ok(Self {
            channel: catalog_entries(&channel),
            global: catalog_entries(&global),
        })
    }

    pub(crate) fn images_for(&self, message_badges: Option<&serde_json::Value>) -> Vec<BadgeImage> {
        message_badges
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|badge| {
                let key = (
                    badge[SET_ID_KEY].as_str()?.to_owned(),
                    badge[VERSION_ID_KEY].as_str()?.to_owned(),
                );
                self.channel
                    .get(&key)
                    .or_else(|| self.global.get(&key))
                    .cloned()
            })
            .collect()
    }
}
