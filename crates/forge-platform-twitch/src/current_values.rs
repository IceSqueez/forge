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
                tracing::warn!(%error, "channel info unreadable; quick action fields not prefilled");
                BTreeMap::new()
            }
        }
    }
}
