use std::collections::BTreeMap;

use forge_net::GatewayRequest;
use forge_platform_core::{PlatformError, QuickActionFieldValue, TokenSource};
use forge_registry::ConfigChoice;
use forge_types::Secret;

use crate::api::data_api::{YouTubeDataApi, malformed};
use crate::broadcast::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::broadcast::stream_metadata::{FETCH_COST, MISSING_SCOPE, VIDEOS_PATH};
use crate::sub_actions::PrivacyStatus;
use crate::vocabulary::config_keys;

pub(crate) async fn read_stream_values(
    api: &YouTubeDataApi,
    token_source: &TokenSource,
    active_broadcast_id: &ActiveBroadcastIdHandle,
    field_keys: &[String],
) -> BTreeMap<String, QuickActionFieldValue> {
    match fetch_stream_values(api, token_source, active_broadcast_id).await {
        Ok(values) => values
            .into_iter()
            .filter(|(key, _)| field_keys.contains(key))
            .map(|(key, value)| (key, QuickActionFieldValue::Text(value)))
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "youtube current stream values unavailable");
            BTreeMap::new()
        }
    }
}

async fn fetch_stream_values(
    api: &YouTubeDataApi,
    token_source: &TokenSource,
    active_broadcast_id: &ActiveBroadcastIdHandle,
) -> Result<BTreeMap<String, String>, PlatformError> {
    let Some(broadcast_id) = active_broadcast_id.resolve_target().await? else {
        return Ok(BTreeMap::new());
    };
    let token = token_source().await?;
    let request = GatewayRequest::get(VIDEOS_PATH)
        .label(VIDEOS_PATH)
        .bearer(Secret::new(token))
        .query("part", "snippet,status")
        .query("id", broadcast_id.as_str());
    let body: serde_json::Value = api
        .send(FETCH_COST, request)
        .await
        .map_err(|error| error.into_platform_error(MISSING_SCOPE))?
        .json()
        .map_err(malformed)?;
    let Some(item) = body
        .get("items")
        .and_then(|items| items.as_array())
        .and_then(|items| items.first())
    else {
        return Ok(BTreeMap::new());
    };
    let text = |section: &str, field: &str| {
        item.get(section)
            .and_then(|part| part.get(field))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let privacy = text("status", "privacyStatus")
        .filter(|status| PrivacyStatus::WIRE_NAMES.contains(&status.as_str()));
    Ok([
        (config_keys::TITLE, text("snippet", "title")),
        (config_keys::DESCRIPTION, text("snippet", "description")),
        (config_keys::CATEGORY_ID, text("snippet", "categoryId")),
        (config_keys::PRIVACY_STATUS, privacy),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value)))
    .collect())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Value, json};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::api::quota_state::SharedQuota;
    use crate::broadcast::broadcast_lookup::BroadcastLookup;
    use crate::endpoints_test_support::{data_api_charging, used_today};

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok("test-token".to_owned()) }))
    }

    fn failing_token_source() -> TokenSource {
        Arc::new(|| {
            Box::pin(async {
                Err(PlatformError::ReauthRequired {
                    platform: "youtube".to_owned(),
                })
            })
        })
    }

    fn all_keys() -> Vec<String> {
        [
            config_keys::TITLE,
            config_keys::DESCRIPTION,
            config_keys::CATEGORY_ID,
            config_keys::PRIVACY_STATUS,
        ]
        .map(str::to_owned)
        .to_vec()
    }

    fn text(value: &str) -> QuickActionFieldValue {
        QuickActionFieldValue::Text(value.to_owned())
    }

    fn video(title: &str, description: &str, category: &str, privacy: &str) -> Value {
        json!({
            "items": [{
                "id": "vid-1",
                "snippet": {
                    "title": title,
                    "description": description,
                    "categoryId": category
                },
                "status": { "privacyStatus": privacy }
            }]
        })
    }

    async fn serving_video(body: Value) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(VIDEOS_PATH))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(body))
            .mount(&server)
            .await;
        server
    }

    fn known_target(id: &str) -> ActiveBroadcastIdHandle {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(Some(id.to_owned()));
        handle
    }

    async fn read_at(
        server: &MockServer,
        handle: &ActiveBroadcastIdHandle,
        keys: &[String],
    ) -> BTreeMap<String, QuickActionFieldValue> {
        let api = data_api_charging(&server.uri(), SharedQuota::default());
        read_stream_values(&api, &token_source(), handle, keys).await
    }

    async fn mount_broadcasts(server: &MockServer, status: &str, items: Value) {
        Mock::given(method("GET"))
            .and(path("/liveBroadcasts"))
            .and(query_param("broadcastStatus", status))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK)
                    .set_body_json(json!({ "items": items })),
            )
            .mount(server)
            .await;
    }

    fn falling_back_on(server: &MockServer) -> ActiveBroadcastIdHandle {
        let handle = ActiveBroadcastIdHandle::new();
        handle.fall_back_to(BroadcastLookup::new(
            data_api_charging(&server.uri(), SharedQuota::default()),
            token_source(),
        ));
        handle
    }

    async fn videos_read_ids(server: &MockServer) -> Vec<String> {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path() == VIDEOS_PATH)
            .filter_map(|request| {
                request
                    .url
                    .query_pairs()
                    .find(|(key, _)| key == "id")
                    .map(|(_, id)| id.into_owned())
            })
            .collect()
    }

    #[tokio::test]
    async fn each_field_reads_its_own_part_of_the_video() {
        let server = serving_video(video("Night stream", "Chill vibes", "24", "unlisted")).await;

        let values = read_at(&server, &known_target("vid-1"), &all_keys()).await;

        assert_eq!(
            values,
            BTreeMap::from([
                (config_keys::TITLE.to_owned(), text("Night stream")),
                (config_keys::DESCRIPTION.to_owned(), text("Chill vibes")),
                (config_keys::CATEGORY_ID.to_owned(), text("24")),
                (config_keys::PRIVACY_STATUS.to_owned(), text("unlisted")),
            ])
        );
    }

    #[tokio::test]
    async fn only_the_requested_keys_are_answered() {
        let server = serving_video(video("Night stream", "Chill vibes", "24", "unlisted")).await;
        let requested = vec![
            config_keys::PRIVACY_STATUS.to_owned(),
            config_keys::TITLE.to_owned(),
        ];

        let values = read_at(&server, &known_target("vid-1"), &requested).await;

        assert_eq!(
            values.keys().map(String::as_str).collect::<Vec<_>>(),
            vec![config_keys::PRIVACY_STATUS, config_keys::TITLE]
        );
    }

    #[tokio::test]
    async fn empty_values_are_left_out() {
        let server = serving_video(video("", "", "", "public")).await;

        let values = read_at(&server, &known_target("vid-1"), &all_keys()).await;

        assert_eq!(
            values,
            BTreeMap::from([(config_keys::PRIVACY_STATUS.to_owned(), text("public"))])
        );
    }

    #[tokio::test]
    async fn a_privacy_value_outside_the_choice_set_is_dropped() {
        let server = serving_video(video("Night stream", "", "", "members_only")).await;

        let values = read_at(&server, &known_target("vid-1"), &all_keys()).await;

        assert_eq!(
            values,
            BTreeMap::from([(config_keys::TITLE.to_owned(), text("Night stream"))])
        );
    }

    #[tokio::test]
    async fn the_live_broadcast_is_read_even_when_one_is_scheduled() {
        let server = serving_video(video("t", "d", "20", "public")).await;
        mount_broadcasts(
            &server,
            "active",
            json!([{ "id": "live-1", "snippet": { "liveChatId": "chat-1" } }]),
        )
        .await;
        mount_broadcasts(
            &server,
            "upcoming",
            json!([{ "id": "next-1", "snippet": { "scheduledStartTime": "2026-11-08T18:00:00Z" } }]),
        )
        .await;

        read_at(&server, &falling_back_on(&server), &all_keys()).await;

        assert_eq!(videos_read_ids(&server).await, vec!["live-1".to_owned()]);
    }

    #[tokio::test]
    async fn without_a_live_broadcast_the_earliest_upcoming_one_is_read() {
        let server = serving_video(video("t", "d", "20", "public")).await;
        mount_broadcasts(&server, "active", json!([])).await;
        mount_broadcasts(
            &server,
            "upcoming",
            json!([
                { "id": "later", "snippet": { "scheduledStartTime": "2026-11-20T18:00:00Z" } },
                { "id": "soonest", "snippet": { "scheduledStartTime": "2026-11-08T18:00:00Z" } }
            ]),
        )
        .await;

        read_at(&server, &falling_back_on(&server), &all_keys()).await;

        assert_eq!(videos_read_ids(&server).await, vec!["soonest".to_owned()]);
    }

    #[tokio::test]
    async fn no_live_or_upcoming_broadcast_reads_nothing_and_skips_the_videos_call() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(VIDEOS_PATH))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK)
                    .set_body_json(video("t", "d", "20", "public")),
            )
            .expect(0)
            .mount(&server)
            .await;
        mount_broadcasts(&server, "active", json!([])).await;
        mount_broadcasts(&server, "upcoming", json!([])).await;

        let values = read_at(&server, &falling_back_on(&server), &all_keys()).await;

        assert!(values.is_empty(), "{values:?}");
        server.verify().await;
    }

    #[tokio::test]
    async fn reading_the_video_charges_its_list_cost_to_the_shared_quota() {
        let server = serving_video(video("t", "d", "20", "public")).await;
        let quota = SharedQuota::default();
        let api = data_api_charging(&server.uri(), quota.clone());

        read_stream_values(&api, &token_source(), &known_target("vid-1"), &all_keys()).await;

        assert_eq!(used_today(&quota).await, FETCH_COST);
    }

    #[tokio::test]
    async fn a_failed_read_answers_an_empty_map() {
        for (label, reply, tokens) in [
            (
                "server error",
                ResponseTemplate::new(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
                token_source(),
            ),
            (
                "undecodable body",
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_string("not json"),
                token_source(),
            ),
            (
                "no token",
                ResponseTemplate::new(reqwest::StatusCode::OK)
                    .set_body_json(video("t", "d", "20", "public")),
                failing_token_source(),
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path(VIDEOS_PATH))
                .respond_with(reply)
                .mount(&server)
                .await;
            let api = data_api_charging(&server.uri(), SharedQuota::default());

            let values =
                read_stream_values(&api, &tokens, &known_target("vid-1"), &all_keys()).await;

            assert!(values.is_empty(), "{label}: {values:?}");
        }
    }
}
