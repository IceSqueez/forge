use async_trait::async_trait;
use forge_platform_core::{FollowLookup, FollowStatus};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::builtin::TwitchIntegrationBundle;
use crate::custom_rewards::first_row;
use crate::helix::{HelixMethod, HelixRequest};

const CHANNEL_FOLLOWERS_PATH: &str = "/helix/channels/followers";
const BROADCASTER_ID_PARAM: &str = "broadcaster_id";
const USER_ID_PARAM: &str = "user_id";
const FIRST_PARAM: &str = "first";
const SINGLE_RESULT: u8 = 1;
const FOLLOWED_AT_KEY: &str = "followed_at";

fn channel_follower_request(broadcaster_id: &str, viewer_id: &str) -> HelixRequest {
    HelixRequest::new(HelixMethod::Get, CHANNEL_FOLLOWERS_PATH)
        .query(BROADCASTER_ID_PARAM, broadcaster_id)
        .query(USER_ID_PARAM, viewer_id)
        .query(FIRST_PARAM, SINGLE_RESULT.to_string())
}

fn status_from_response(response: &serde_json::Value) -> FollowStatus {
    let Some(row) = first_row(response) else {
        return FollowStatus::NotFollowing;
    };
    row[FOLLOWED_AT_KEY]
        .as_str()
        .and_then(|raw| OffsetDateTime::parse(raw, &Rfc3339).ok())
        .map_or(FollowStatus::Unavailable, FollowStatus::FollowedSince)
}

#[async_trait]
impl FollowLookup for TwitchIntegrationBundle {
    async fn follow_status(&self, viewer_id: &str) -> FollowStatus {
        let broadcaster_id = self.broadcaster_id();
        if broadcaster_id.is_empty() || viewer_id.is_empty() {
            return FollowStatus::Unavailable;
        }
        if viewer_id == broadcaster_id {
            return FollowStatus::NotFollowing;
        }
        match self
            .helix()
            .execute(channel_follower_request(broadcaster_id, viewer_id))
            .await
        {
            Ok(response) => status_from_response(&response),
            Err(_) => FollowStatus::Unavailable,
        }
    }
}
