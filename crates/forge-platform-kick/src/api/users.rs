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

#[cfg(test)]
mod tests {
    use forge_storage::test_support::MemoryCredentials;
    use forge_types::OAuthToken;
    use serde_json::json;
    use time::{Duration, OffsetDateTime};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    use super::*;
    use crate::auth::credentials::{KICK_CREDENTIAL_ID, KickCredentials};
    use crate::test_support::{api_at, capture_blocking, offline_manager};

    const ACCESS_TOKEN: &str = "FAKE_KICK_ACCESS_TOKEN_SENTINEL_k7q2";

    fn ids(viewer_ids: &[&str]) -> Vec<String> {
        viewer_ids.iter().map(|id| (*id).to_owned()).collect()
    }

    fn no_users() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({ "data": [] }))
    }

    async fn kick_answering(response: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(USERS_PATH))
            .respond_with(response)
            .mount(&server)
            .await;
        server
    }

    fn signed_in() -> Arc<MemoryCredentials> {
        let creds = KickCredentials {
            access_token: OAuthToken::new(ACCESS_TOKEN),
            refresh_token: OAuthToken::new("FAKE_KICK_REFRESH_TOKEN"),
            user_id: 42,
            username: "streamer".to_owned(),
            client_id: "cid".to_owned(),
            expires_at: OffsetDateTime::now_utc() + Duration::hours(1),
        };
        Arc::new(MemoryCredentials::holding(
            KICK_CREDENTIAL_ID,
            &serde_json::to_string(&creds).unwrap_or_default(),
        ))
    }

    fn source_at(server: &MockServer) -> KickViewerAvatars {
        KickViewerAvatars::new(
            api_at(&server.uri()),
            Arc::new(offline_manager(signed_in())),
        )
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
    async fn every_numeric_viewer_is_asked_for_as_its_own_id_parameter() {
        let server = kick_answering(no_users()).await;

        source_at(&server)
            .avatars(&ids(&["11", "not-a-number", " 22 ", ""]))
            .await;

        let sent = requests(&server).await;
        assert_eq!(sent.len(), 1);
        assert_eq!(id_params(&sent[0]), ids(&["11", "22"]));
    }

    #[tokio::test]
    async fn one_lookup_asks_for_at_most_fifty_viewers() {
        let server = kick_answering(no_users()).await;
        let viewers: Vec<String> = (1..=USERS_PER_REQUEST + 1).map(|n| n.to_string()).collect();

        source_at(&server).avatars(&viewers).await;

        let sent = requests(&server).await;
        assert_eq!(id_params(&sent[0]), viewers[..USERS_PER_REQUEST].to_vec());
    }

    #[tokio::test]
    async fn no_numeric_viewer_is_answered_empty_without_calling_kick() {
        let server = kick_answering(no_users()).await;

        let lookup = source_at(&server).avatars(&ids(&["ada", ""])).await;

        assert_eq!(lookup, AvatarLookup::Answered(HashMap::new()));
        assert!(requests(&server).await.is_empty());
    }

    #[tokio::test]
    async fn the_users_answer_maps_each_id_to_its_profile_picture_skipping_incomplete_rows() {
        let server = kick_answering(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                { "user_id": 11, "name": "ada", "profile_picture": "https://files.kick.com/a.webp" },
                { "user_id": 22, "name": "bob", "profile_picture": "" },
                { "user_id": 33, "name": "cyd", "profile_picture": null },
                { "name": "dan", "profile_picture": "https://files.kick.com/d.webp" }
            ]
        })))
        .await;

        let lookup = source_at(&server).avatars(&ids(&["11", "22", "33"])).await;

        assert_eq!(
            lookup,
            AvatarLookup::Answered(HashMap::from([(
                "11".to_owned(),
                "https://files.kick.com/a.webp".to_owned()
            )]))
        );
    }

    #[tokio::test]
    async fn a_refused_or_unreadable_lookup_is_unavailable() {
        for response in [
            ResponseTemplate::new(401),
            ResponseTemplate::new(403),
            ResponseTemplate::new(429),
            ResponseTemplate::new(200).set_body_string("not json"),
        ] {
            let server = kick_answering(response).await;

            let lookup = source_at(&server).avatars(&ids(&["11"])).await;

            assert_eq!(lookup, AvatarLookup::Unavailable);
        }
    }

    #[test]
    fn a_refused_lookup_never_logs_the_token() {
        for status in [401_u16, 403, 429, 500] {
            let (lookup, lines) = capture_blocking(tracing::Level::TRACE, async {
                let server = kick_answering(ResponseTemplate::new(status)).await;
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
                lines.iter().all(|line| !line.mentions(ACCESS_TOKEN)),
                "status {status} logged the token: {lines:?}"
            );
        }
    }
}
