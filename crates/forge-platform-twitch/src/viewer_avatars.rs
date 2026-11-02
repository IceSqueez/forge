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

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    use super::*;
    use crate::test_support::{TOKEN_SENTINEL, observed_helix_at};

    fn ids(viewer_ids: &[&str]) -> Vec<String> {
        viewer_ids.iter().map(|id| (*id).to_owned()).collect()
    }

    fn no_users() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({ "data": [] }))
    }

    async fn helix_answering(response: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(USERS_PATH))
            .respond_with(response)
            .mount(&server)
            .await;
        server
    }

    fn source_at(server: &MockServer) -> TwitchViewerAvatars {
        let (transport, _bus) = observed_helix_at(&server.uri());
        TwitchViewerAvatars::new(transport)
    }

    fn id_params(request: &Request) -> Vec<String> {
        request
            .url
            .query_pairs()
            .filter(|(key, _)| key == ID_PARAM)
            .map(|(_, value)| value.into_owned())
            .collect()
    }

    async fn requests(server: &MockServer) -> Vec<Request> {
        server.received_requests().await.unwrap_or_default()
    }

    #[tokio::test]
    async fn every_viewer_is_asked_for_as_its_own_id_parameter_skipping_blank_ids() {
        let server = helix_answering(no_users()).await;

        source_at(&server).avatars(&ids(&["11", "", "22"])).await;

        let sent = requests(&server).await;
        assert_eq!(sent.len(), 1);
        assert_eq!(id_params(&sent[0]), ids(&["11", "22"]));
    }

    #[tokio::test]
    async fn one_lookup_asks_for_at_most_a_hundred_viewers() {
        let server = helix_answering(no_users()).await;
        let viewers: Vec<String> = (0..=USERS_PER_REQUEST).map(|n| n.to_string()).collect();

        source_at(&server).avatars(&viewers).await;

        let sent = requests(&server).await;
        assert_eq!(id_params(&sent[0]), viewers[..USERS_PER_REQUEST].to_vec());
    }

    #[tokio::test]
    async fn only_blank_ids_are_answered_empty_without_calling_helix() {
        let server = helix_answering(no_users()).await;

        let lookup = source_at(&server).avatars(&ids(&["", ""])).await;

        assert_eq!(lookup, AvatarLookup::Answered(HashMap::new()));
        assert!(requests(&server).await.is_empty());
    }

    #[tokio::test]
    async fn the_users_answer_maps_each_id_to_its_profile_image_skipping_incomplete_rows() {
        let server = helix_answering(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                { "id": "11", "login": "ada", "profile_image_url": "https://static-cdn.jtvnw.net/a.png" },
                { "id": "22", "login": "bob", "profile_image_url": "" },
                { "id": "", "login": "eve", "profile_image_url": "https://static-cdn.jtvnw.net/e.png" },
                { "id": "33", "login": "cyd" },
                { "login": "dan", "profile_image_url": "https://static-cdn.jtvnw.net/d.png" }
            ]
        })))
        .await;

        let lookup = source_at(&server).avatars(&ids(&["11", "22", "33"])).await;

        assert_eq!(
            lookup,
            AvatarLookup::Answered(HashMap::from([(
                "11".to_owned(),
                "https://static-cdn.jtvnw.net/a.png".to_owned()
            )]))
        );
    }

    #[tokio::test]
    async fn a_refused_lookup_is_unavailable() {
        for status in [401_u16, 403, 429] {
            let server = helix_answering(ResponseTemplate::new(status)).await;

            let lookup = source_at(&server).avatars(&ids(&["11"])).await;

            assert_eq!(lookup, AvatarLookup::Unavailable, "status {status}");
        }
    }

    #[test]
    fn a_refused_lookup_never_logs_the_token() {
        for status in [401_u16, 403, 429, 500] {
            let (lookup, lines) =
                forge_test_support::log_capture::capture_blocking(tracing::Level::TRACE, async {
                    let server = helix_answering(ResponseTemplate::new(status)).await;
                    source_at(&server).avatars(&ids(&["11"])).await
                });

            assert_eq!(lookup, AvatarLookup::Unavailable, "status {status}");
            assert!(
                lines
                    .iter()
                    .any(|line| line.message().contains("viewer avatars unavailable")),
                "status {status} left no line to audit"
            );
            assert!(
                lines.iter().all(|line| !line.mentions(TOKEN_SENTINEL)),
                "status {status} logged the token: {lines:?}"
            );
        }
    }
}
