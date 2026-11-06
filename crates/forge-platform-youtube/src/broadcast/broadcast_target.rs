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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn scheduled(id: &str, start: &str) -> Value {
        json!({ "id": id, "snippet": { "scheduledStartTime": start } })
    }

    fn listing(items: Vec<Value>) -> Value {
        json!({ "items": items })
    }

    #[test]
    fn the_soonest_scheduled_start_wins_whatever_the_listing_order() {
        let body = listing(vec![
            scheduled("late", "2026-11-20T18:00:00Z"),
            scheduled("soonest", "2026-11-08T18:00:00Z"),
            scheduled("middle", "2026-11-12T18:00:00Z"),
        ]);

        assert_eq!(earliest_scheduled_id(&body).as_deref(), Some("soonest"));
    }

    #[test]
    fn start_times_compare_as_instants_not_as_offset_text() {
        let body = listing(vec![
            scheduled("nine_utc", "2026-11-08T09:00:00Z"),
            scheduled("eight_utc", "2026-11-08T10:00:00+02:00"),
        ]);

        assert_eq!(earliest_scheduled_id(&body).as_deref(), Some("eight_utc"));
    }

    #[test]
    fn items_without_a_readable_start_or_id_are_skipped() {
        for (label, unreadable) in [
            ("no snippet", json!({ "id": "bad" })),
            (
                "no start",
                json!({ "id": "bad", "snippet": { "title": "t" } }),
            ),
            ("unparseable start", scheduled("bad", "tomorrow evening")),
            ("empty start", scheduled("bad", "")),
            (
                "numeric start",
                json!({ "id": "bad", "snippet": { "scheduledStartTime": 0 } }),
            ),
            (
                "no id",
                json!({ "snippet": { "scheduledStartTime": "2026-11-01T00:00:00Z" } }),
            ),
        ] {
            let body = listing(vec![unreadable, scheduled("good", "2026-11-09T18:00:00Z")]);

            assert_eq!(
                earliest_scheduled_id(&body).as_deref(),
                Some("good"),
                "{label}"
            );
        }
    }

    #[test]
    fn a_listing_with_no_scheduled_item_has_no_target() {
        for (label, body) in [
            ("no items key", json!({})),
            ("items not a list", json!({ "items": {} })),
            ("empty items", listing(vec![])),
            ("only unscheduled", listing(vec![json!({ "id": "x" })])),
        ] {
            assert_eq!(earliest_scheduled_id(&body), None, "{label}");
        }
    }
}
