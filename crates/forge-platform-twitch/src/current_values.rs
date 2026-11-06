use std::collections::BTreeMap;

use async_trait::async_trait;
use forge_platform_core::{BuiltinCurrentValues, QuickActionFieldValue};
use serde_json::Value;

use crate::builtin::TwitchIntegrationBundle;
use crate::helix::{HelixMethod, HelixRequest};
use crate::helix_envelope::first_row;
use crate::vocabulary::config_keys;

const CHANNELS_PATH: &str = "/helix/channels";
const BROADCASTER_ID_PARAM: &str = "broadcaster_id";
const TITLE_KEY: &str = "title";
const GAME_NAME_KEY: &str = "game_name";
const TAGS_KEY: &str = "tags";
const TAG_SEPARATOR: &str = ", ";

fn text_of(row: &Value, key: &str) -> Option<String> {
    row[key]
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn joined_tags(row: &Value) -> Option<String> {
    let tags: Vec<&str> = row[TAGS_KEY]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    (!tags.is_empty()).then(|| tags.join(TAG_SEPARATOR))
}

fn value_for(row: &Value, field_key: &str) -> Option<String> {
    match field_key {
        config_keys::TITLE => text_of(row, TITLE_KEY),
        config_keys::CATEGORY_NAME => text_of(row, GAME_NAME_KEY),
        config_keys::TAGS => joined_tags(row),
        _ => None,
    }
}

fn values_from_response(
    response: &Value,
    field_keys: &[String],
) -> BTreeMap<String, QuickActionFieldValue> {
    let Some(row) = first_row(response) else {
        return BTreeMap::new();
    };
    field_keys
        .iter()
        .filter_map(|key| {
            value_for(row, key).map(|text| (key.clone(), QuickActionFieldValue::Text(text)))
        })
        .collect()
}

#[async_trait]
impl BuiltinCurrentValues for TwitchIntegrationBundle {
    async fn read_current(&self, field_keys: &[String]) -> BTreeMap<String, QuickActionFieldValue> {
        let request = HelixRequest::new(HelixMethod::Get, CHANNELS_PATH)
            .query(BROADCASTER_ID_PARAM, self.broadcaster_id());
        match self.helix().execute(request).await {
            Ok(response) => values_from_response(&response, field_keys),
            Err(error) => {
                tracing::debug!(%error, "channel info unreadable; quick action fields not prefilled");
                BTreeMap::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_types::{ArgStack, SubActionOutcome, Variant};
    use serde_json::json;
    use tokio::sync::watch;

    use super::*;
    use crate::BroadcasterTier;
    use crate::chat::ChatSessionState;
    use crate::error::TwitchError;
    use crate::helix::HelixTransport;
    use crate::sub_actions::UpdateTagsRunner;
    use crate::subscriptions::SubscriptionTracker;
    use crate::test_support::{MockCreds, MockTransport, make_ctx, runner_with};
    use forge_registry::SubActionRunner;

    const BROADCASTER: &str = "100000001";

    fn bundle_answering(
        reply: Result<Value, TwitchError>,
    ) -> (Arc<MockTransport>, Arc<TwitchIntegrationBundle>) {
        let transport = Arc::new(MockTransport::returning(reply));
        let (_tx, rx) = watch::channel(ChatSessionState::Connected);
        let bundle = TwitchIntegrationBundle::for_test_with_transport(
            Some("streamer".to_owned()),
            rx,
            SubscriptionTracker::default(),
            Arc::new(MockCreds::with_identity()),
            BroadcasterTier::Affiliate,
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            BROADCASTER,
        );
        (transport, bundle)
    }

    fn channel(title: &str, game_name: &str, tags: &[&str]) -> Value {
        json!({
            "data": [{
                "broadcaster_id": BROADCASTER,
                "title": title,
                "game_id": "509658",
                "game_name": game_name,
                "tags": tags
            }]
        })
    }

    fn keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|key| (*key).to_owned()).collect()
    }

    fn all_keys() -> Vec<String> {
        keys(&[
            config_keys::TITLE,
            config_keys::CATEGORY_NAME,
            config_keys::TAGS,
        ])
    }

    fn text(value: &str) -> QuickActionFieldValue {
        QuickActionFieldValue::Text(value.to_owned())
    }

    async fn read(
        reply: Result<Value, TwitchError>,
        requested: &[String],
    ) -> BTreeMap<String, QuickActionFieldValue> {
        let (_transport, bundle) = bundle_answering(reply);
        bundle.read_current(requested).await
    }

    #[tokio::test]
    async fn the_channel_of_this_broadcaster_is_read() {
        let (transport, bundle) = bundle_answering(Ok(channel("t", "g", &[])));

        bundle.read_current(&all_keys()).await;

        let request = transport.request(0);
        assert_eq!(
            (request.method, request.path, request.query),
            (
                HelixMethod::Get,
                CHANNELS_PATH,
                vec![(BROADCASTER_ID_PARAM.to_owned(), BROADCASTER.to_owned())]
            )
        );
    }

    #[tokio::test]
    async fn the_title_and_the_category_name_fill_their_fields() {
        let values = read(
            Ok(channel("Speedrun night", "Just Chatting", &[])),
            &keys(&[config_keys::TITLE, config_keys::CATEGORY_NAME]),
        )
        .await;

        assert_eq!(
            values,
            BTreeMap::from([
                (config_keys::TITLE.to_owned(), text("Speedrun night")),
                (config_keys::CATEGORY_NAME.to_owned(), text("Just Chatting")),
            ])
        );
    }

    #[tokio::test]
    async fn only_the_requested_keys_are_answered() {
        let values = read(
            Ok(channel("Speedrun night", "Just Chatting", &["English"])),
            &keys(&[config_keys::TAGS, "message"]),
        )
        .await;

        assert_eq!(
            values.keys().map(String::as_str).collect::<Vec<_>>(),
            vec![config_keys::TAGS]
        );
    }

    #[tokio::test]
    async fn empty_values_are_left_out() {
        let values = read(Ok(channel("", "", &[])), &all_keys()).await;

        assert!(values.is_empty(), "{values:?}");
    }

    #[tokio::test]
    async fn a_failed_or_rowless_read_answers_an_empty_map() {
        for (label, reply) in [
            (
                "helix error",
                Err(TwitchError::Http {
                    status: 500,
                    body: String::new(),
                }),
            ),
            ("no rows", Ok(json!({ "data": [] }))),
        ] {
            let values = read(reply, &all_keys()).await;

            assert!(values.is_empty(), "{label}: {values:?}");
        }
    }

    #[tokio::test]
    async fn the_read_tags_parse_back_into_the_same_list_in_the_update_tags_step() {
        let tags = ["Speedrun", "English", "Українська", "PvP"];
        let values = read(Ok(channel("t", "g", &tags)), &all_keys()).await;
        let Some(QuickActionFieldValue::Text(field_text)) = values.get(config_keys::TAGS) else {
            panic!("tags not read: {values:?}");
        };
        let (transport, runner) = runner_with(vec![Ok(Value::Null)], UpdateTagsRunner::new);
        let config = BTreeMap::from([(
            config_keys::TAGS.to_owned(),
            Variant::String(field_text.clone()),
        )]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config, &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(transport.request(0).body, Some(json!({ "tags": tags })));
    }
}
