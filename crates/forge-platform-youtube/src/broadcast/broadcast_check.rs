use forge_net::GatewayRequest;
use forge_platform_core::ViewerReport;
use forge_types::Secret;
use tokio_util::sync::CancellationToken;

use crate::api::data_api::{DataApiError, YouTubeDataApi, malformed};

const VIDEOS_PATH: &str = "/videos";
const VIDEO_CHECK_COST: u32 = 1;
const VIDEO_CHECK_PARTS: &str = "snippet,liveStreamingDetails";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CheckedChat {
    Open { live_chat_id: String },
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BroadcastSnapshot {
    pub(crate) title: Option<String>,
    pub(crate) viewers: ViewerReport,
    pub(crate) chat: CheckedChat,
}

pub(crate) enum CheckFailure {
    Api(DataApiError),
    Malformed(forge_platform_core::PlatformError),
}

pub(crate) async fn check_broadcast(
    api: &YouTubeDataApi,
    token: &str,
    broadcast_id: &str,
    cancel: &CancellationToken,
) -> Result<BroadcastSnapshot, CheckFailure> {
    let request = GatewayRequest::get(VIDEOS_PATH)
        .label(VIDEOS_PATH)
        .bearer(Secret::new(token))
        .query("part", VIDEO_CHECK_PARTS)
        .query("id", broadcast_id)
        .cancel_on(cancel.clone());
    let body: serde_json::Value = api
        .send(VIDEO_CHECK_COST, request)
        .await
        .map_err(CheckFailure::Api)?
        .json()
        .map_err(|error| CheckFailure::Malformed(malformed(error)))?;
    Ok(snapshot_of(&body))
}

fn snapshot_of(body: &serde_json::Value) -> BroadcastSnapshot {
    let item = body
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| items.first());
    let title = item
        .and_then(|item| item.get("snippet"))
        .and_then(|snippet| snippet.get("title"))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let details = item.and_then(|item| item.get("liveStreamingDetails"));
    let viewers = details
        .and_then(|details| details.get("concurrentViewers"))
        .and_then(serde_json::Value::as_str)
        .and_then(|count| count.parse::<u64>().ok())
        .map_or(ViewerReport::Absent, |count| ViewerReport::Live { count });
    let ended = details.is_none_or(|details| details.get("actualEndTime").is_some());
    let live_chat_id = details
        .and_then(|details| details.get("activeLiveChatId"))
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.is_empty());
    let chat = match live_chat_id {
        Some(id) if !ended => CheckedChat::Open {
            live_chat_id: id.to_owned(),
        },
        _ => CheckedChat::Ended,
    };
    BroadcastSnapshot {
        title,
        viewers,
        chat,
    }
}
