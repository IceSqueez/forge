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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockTransport;
    use serde_json::json;

    const GLOBAL_SUB: &str = "https://static-cdn.jtvnw.net/badges/v1/global-sub/2";
    const CHANNEL_SUB: &str = "https://static-cdn.jtvnw.net/badges/v1/channel-sub/2";
    const GLOBAL_MOD: &str = "https://static-cdn.jtvnw.net/badges/v1/global-mod/2";

    fn set(set_id: &str, versions: serde_json::Value) -> serde_json::Value {
        json!({ "set_id": set_id, "versions": versions })
    }

    fn version(id: &str, title: &str, url: &str) -> serde_json::Value {
        json!({ "id": id, "title": title, "image_url_2x": url })
    }

    fn global_sets() -> serde_json::Value {
        json!({ "data": [
            set("subscriber", json!([version("0", "Subscriber", GLOBAL_SUB)])),
            set("moderator", json!([version("1", "Moderator", GLOBAL_MOD)])),
        ] })
    }

    fn channel_sets() -> serde_json::Value {
        json!({ "data": [
            set("subscriber", json!([version("0", "Channel Sub", CHANNEL_SUB)])),
        ] })
    }

    async fn fetched(
        responses: Vec<Result<serde_json::Value, TwitchError>>,
    ) -> (Arc<MockTransport>, Result<BadgeCatalog, TwitchError>) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let helix: Arc<dyn HelixTransport> = transport.clone();
        let catalog = BadgeCatalog::fetch(&helix, "1001").await;
        (transport, catalog)
    }

    fn badges(pairs: &[(&str, &str)]) -> serde_json::Value {
        pairs
            .iter()
            .map(|(set_id, id)| json!({ "set_id": set_id, "id": id }))
            .collect()
    }

    fn urls(images: Vec<BadgeImage>) -> Vec<String> {
        images.into_iter().map(|image| image.url).collect()
    }

    #[tokio::test]
    async fn the_catalog_asks_for_the_global_set_then_the_streamers_own_channel_set() {
        let (transport, catalog) = fetched(vec![Ok(global_sets()), Ok(channel_sets())]).await;

        assert!(catalog.is_ok());
        assert_eq!(transport.call_count(), 2);
        assert_eq!(transport.request(0).path, GLOBAL_BADGES_PATH);
        assert!(transport.request(0).query.is_empty());
        assert_eq!(transport.request(1).path, CHANNEL_BADGES_PATH);
        assert_eq!(
            transport.request(1).query,
            vec![(BROADCASTER_PARAM.to_owned(), "1001".to_owned())]
        );
    }

    #[tokio::test]
    async fn a_channel_badge_wins_over_the_global_badge_with_the_same_set_and_version() {
        let (_, catalog) = fetched(vec![Ok(global_sets()), Ok(channel_sets())]).await;
        let catalog = catalog.expect("both sets load");

        let images = catalog.images_for(Some(&badges(&[("moderator", "1"), ("subscriber", "0")])));

        assert_eq!(
            images,
            vec![
                BadgeImage {
                    title: "Moderator".to_owned(),
                    url: GLOBAL_MOD.to_owned()
                },
                BadgeImage {
                    title: "Channel Sub".to_owned(),
                    url: CHANNEL_SUB.to_owned()
                },
            ]
        );
    }

    #[tokio::test]
    async fn a_badge_the_catalog_does_not_know_or_cannot_read_is_left_out() {
        let (_, catalog) = fetched(vec![Ok(global_sets()), Ok(channel_sets())]).await;
        let catalog = catalog.expect("both sets load");
        let message = json!([
            { "set_id": "subscriber", "id": "12" },
            { "set_id": "vip", "id": "1" },
            { "set_id": "moderator" },
            { "id": "1" },
            "moderator",
            { "set_id": "moderator", "id": "1" },
        ]);

        assert_eq!(urls(catalog.images_for(Some(&message))), vec![GLOBAL_MOD]);
        assert!(catalog.images_for(None).is_empty());
        assert!(
            catalog
                .images_for(Some(&json!({ "set_id": "moderator" })))
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_catalog_entry_without_a_set_or_a_picture_is_never_offered() {
        let global = json!({ "data": [
            set("", json!([version("1", "Nameless", GLOBAL_MOD)])),
            set("moderator", json!([version("1", "Moderator", "")])),
            set("vip", json!([{ "id": "1", "title": "VIP" }])),
            set("partner", json!([{ "title": "Partner", "image_url_2x": GLOBAL_SUB }])),
            set("staff", json!([{ "id": "1", "image_url_2x": GLOBAL_SUB }])),
        ] });
        let (_, catalog) = fetched(vec![Ok(global), Ok(json!({ "data": [] }))]).await;
        let catalog = catalog.expect("both sets load");

        let images = catalog.images_for(Some(&badges(&[
            ("", "1"),
            ("moderator", "1"),
            ("vip", "1"),
            ("staff", "1"),
        ])));

        assert_eq!(
            images,
            vec![BadgeImage {
                title: "staff".to_owned(),
                url: GLOBAL_SUB.to_owned()
            }]
        );
    }

    #[tokio::test]
    async fn a_refused_badge_lookup_fails_the_whole_catalog() {
        let refused = || {
            Err(TwitchError::Http {
                status: 500,
                body: String::new(),
            })
        };
        for (case, responses) in [
            ("global refused", vec![refused(), Ok(channel_sets())]),
            ("channel refused", vec![Ok(global_sets()), refused()]),
        ] {
            let (_, catalog) = fetched(responses).await;
            assert!(catalog.is_err(), "{case}");
        }
    }
}
