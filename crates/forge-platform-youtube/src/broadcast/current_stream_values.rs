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
