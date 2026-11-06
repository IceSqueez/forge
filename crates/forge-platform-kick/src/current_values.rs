use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::{BuiltinCurrentValues, QuickActionFieldValue};

use crate::api::channel::{ChannelSnapshot, KickChannel};
use crate::auth::credentials_manager::KickCredentialsManager;
use crate::vocabulary::config_keys;

pub struct KickCurrentValues {
    channel: Arc<KickChannel>,
    credentials_manager: Arc<KickCredentialsManager>,
}

impl KickCurrentValues {
    pub fn new(
        channel: Arc<KickChannel>,
        credentials_manager: Arc<KickCredentialsManager>,
    ) -> Self {
        Self {
            channel,
            credentials_manager,
        }
    }

    async fn snapshot(&self) -> Option<ChannelSnapshot> {
        let token = match self.credentials_manager.get_valid_access_token().await {
            Ok(token) => token,
            Err(error) => {
                tracing::debug!(%error, "kick token unavailable for current values");
                return None;
            }
        };
        match self.channel.get_channel(&token).await {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                tracing::debug!(%error, "kick channel read failed for current values");
                None
            }
        }
    }
}

fn value_of(key: &str, snapshot: &ChannelSnapshot) -> Option<QuickActionFieldValue> {
    match key {
        config_keys::TITLE if !snapshot.stream_title.is_empty() => {
            Some(QuickActionFieldValue::Text(snapshot.stream_title.clone()))
        }
        config_keys::CATEGORY_ID => snapshot
            .category_id
            .map(|id| QuickActionFieldValue::Text(id.to_string())),
        _ => None,
    }
}

#[async_trait]
impl BuiltinCurrentValues for KickCurrentValues {
    async fn read_current(&self, field_keys: &[String]) -> BTreeMap<String, QuickActionFieldValue> {
        let Some(snapshot) = self.snapshot().await else {
            return BTreeMap::new();
        };
        field_keys
            .iter()
            .filter_map(|key| Some((key.clone(), value_of(key, &snapshot)?)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::test_support::MemoryCredentials;
    use forge_types::OAuthToken;
    use serde_json::{Value, json};
    use time::{Duration, OffsetDateTime};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::auth::credentials::{KICK_CREDENTIAL_ID, KickCredentials};
    use crate::test_support::{api_at, offline_manager};

    const CHANNELS_PATH: &str = "/channels";

    fn signed_in() -> Arc<MemoryCredentials> {
        let creds = KickCredentials {
            access_token: OAuthToken::new("FAKE_KICK_ACCESS_TOKEN"),
            refresh_token: OAuthToken::new("FAKE_KICK_REFRESH_TOKEN"),
            user_id: 42,
            username: "streamer".to_owned(),
            client_id: "cid".to_owned(),
            expires_at: OffsetDateTime::now_utc() + Duration::hours(1),
        };
        Arc::new(MemoryCredentials::holding(
            KICK_CREDENTIAL_ID,
            &serde_json::to_string(&creds).unwrap(),
        ))
    }

    fn channel(title: &str, category: Value) -> Value {
        json!({
            "data": [{
                "broadcaster_user_id": 42,
                "slug": "streamer",
                "stream_title": title,
                "category": category
            }]
        })
    }

    async fn serving(reply: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(CHANNELS_PATH))
            .respond_with(reply)
            .mount(&server)
            .await;
        server
    }

    fn reader_at(server: &MockServer, credentials: Arc<MemoryCredentials>) -> KickCurrentValues {
        KickCurrentValues::new(
            Arc::new(KickChannel::new(api_at(&server.uri()))),
            Arc::new(offline_manager(credentials)),
        )
    }

    async fn read(
        channel_body: Value,
        requested: &[&str],
    ) -> BTreeMap<String, QuickActionFieldValue> {
        let server =
            serving(ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(channel_body))
                .await;
        let requested: Vec<String> = requested.iter().map(|key| (*key).to_owned()).collect();
        reader_at(&server, signed_in())
            .read_current(&requested)
            .await
    }

    fn text(value: &str) -> QuickActionFieldValue {
        QuickActionFieldValue::Text(value.to_owned())
    }

    #[tokio::test]
    async fn the_title_and_the_category_id_as_its_number_text_fill_their_fields() {
        let values = read(
            channel(
                "Speedrun night",
                json!({ "id": 4_294_967_301_u64, "name": "Just Chatting" }),
            ),
            &[config_keys::TITLE, config_keys::CATEGORY_ID],
        )
        .await;

        assert_eq!(
            values,
            BTreeMap::from([
                (config_keys::TITLE.to_owned(), text("Speedrun night")),
                (config_keys::CATEGORY_ID.to_owned(), text("4294967301")),
            ])
        );
    }

    #[tokio::test]
    async fn only_the_requested_keys_are_answered() {
        let values = read(
            channel(
                "Speedrun night",
                json!({ "id": 77, "name": "Just Chatting" }),
            ),
            &[config_keys::CATEGORY_ID, config_keys::TAGS],
        )
        .await;

        assert_eq!(
            values.keys().map(String::as_str).collect::<Vec<_>>(),
            vec![config_keys::CATEGORY_ID]
        );
    }

    #[tokio::test]
    async fn an_empty_title_and_a_missing_category_are_left_out() {
        let values = read(
            channel("", json!({ "name": "" })),
            &[config_keys::TITLE, config_keys::CATEGORY_ID],
        )
        .await;

        assert!(values.is_empty(), "{values:?}");
    }

    #[tokio::test]
    async fn a_failed_channel_read_answers_an_empty_map() {
        for (label, reply) in [
            (
                "server error",
                ResponseTemplate::new(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
            ),
            (
                "no channel row",
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(json!({ "data": [] })),
            ),
        ] {
            let server = serving(reply).await;

            let values = reader_at(&server, signed_in())
                .read_current(&[config_keys::TITLE.to_owned()])
                .await;

            assert!(values.is_empty(), "{label}: {values:?}");
        }
    }

    #[tokio::test]
    async fn signed_out_reads_nothing_and_never_asks_for_the_channel() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(CHANNELS_PATH))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK)
                    .set_body_json(channel("Speedrun night", json!({ "id": 77 }))),
            )
            .expect(0)
            .mount(&server)
            .await;

        let values = reader_at(&server, Arc::new(MemoryCredentials::default()))
            .read_current(&[config_keys::TITLE.to_owned()])
            .await;

        assert!(values.is_empty(), "{values:?}");
        server.verify().await;
    }
}
