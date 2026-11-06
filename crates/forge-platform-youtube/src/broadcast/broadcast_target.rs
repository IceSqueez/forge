use forge_net::GatewayRequest;
use forge_types::Secret;
use jiff::Timestamp;

use crate::api::data_api::{YouTubeDataApi, malformed};
use crate::api::quota_state::BROADCAST_COST;
use crate::broadcast::broadcast_lookup::{BROADCASTS_PATH, LookupFailure};

const UPCOMING_PAGE_SIZE: &str = "50";

pub(crate) async fn fetch_earliest_upcoming_id(
    api: &YouTubeDataApi,
    token: &str,
) -> Result<Option<String>, LookupFailure> {
    let request = GatewayRequest::get(BROADCASTS_PATH)
        .label(BROADCASTS_PATH)
        .bearer(Secret::new(token))
        .query("part", "snippet")
        .query("broadcastStatus", "upcoming")
        .query("maxResults", UPCOMING_PAGE_SIZE);
    let body: serde_json::Value = api
        .send(BROADCAST_COST, request)
        .await
        .map_err(LookupFailure::Api)?
        .json()
        .map_err(|error| LookupFailure::Malformed(malformed(error)))?;
    Ok(earliest_scheduled_id(&body))
}

fn earliest_scheduled_id(body: &serde_json::Value) -> Option<String> {
    body.get("items")?
        .as_array()?
        .iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?;
            let start = item
                .get("snippet")?
                .get("scheduledStartTime")?
                .as_str()?
                .parse::<Timestamp>()
                .ok()?;
            Some((start, id))
        })
        .min_by_key(|(start, _)| *start)
        .map(|(_, id)| id.to_owned())
}
