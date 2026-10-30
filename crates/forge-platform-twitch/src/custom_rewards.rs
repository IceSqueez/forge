use crate::helix::{HelixMethod, HelixRequest};

pub(crate) const CUSTOM_REWARDS_PATH: &str = "/helix/channel_points/custom_rewards";
pub(crate) const CUSTOM_REWARD_REDEMPTIONS_PATH: &str =
    "/helix/channel_points/custom_rewards/redemptions";
const REDEMPTIONS_PAGE_SIZE: usize = 50;
pub(crate) const MAX_CUSTOM_REWARDS: usize = 50;
pub(crate) const MAX_TITLE_CHARS: usize = 45;
pub(crate) const MAX_PROMPT_CHARS: usize = 200;
pub(crate) const MIN_COST: i64 = 1;
pub(crate) const HEX_COLOR_LEN: usize = 7;
const HEX_COLOR_PREFIX: char = '#';

const BROADCASTER_ID_PARAM: &str = "broadcaster_id";
const REWARD_ID_PARAM: &str = "id";
const ONLY_MANAGEABLE_PARAM: &str = "only_manageable_rewards";
const REDEEMED_REWARD_ID_PARAM: &str = "reward_id";
const STATUS_PARAM: &str = "status";
const STATUS_UNFULFILLED: &str = "UNFULFILLED";
const FIRST_PARAM: &str = "first";

pub(crate) const TITLE_KEY: &str = "title";
pub(crate) const COST_KEY: &str = "cost";
pub(crate) const PROMPT_KEY: &str = "prompt";
pub(crate) const BACKGROUND_COLOR_KEY: &str = "background_color";
pub(crate) const IS_ENABLED_KEY: &str = "is_enabled";
pub(crate) const IS_PAUSED_KEY: &str = "is_paused";
pub(crate) const IS_USER_INPUT_REQUIRED_KEY: &str = "is_user_input_required";
pub(crate) const SKIP_REQUEST_QUEUE_KEY: &str = "should_redemptions_skip_request_queue";
pub(crate) const MAX_PER_STREAM_KEY: &str = "max_per_stream";
pub(crate) const MAX_PER_USER_PER_STREAM_KEY: &str = "max_per_user_per_stream";
pub(crate) const GLOBAL_COOLDOWN_SECONDS_KEY: &str = "global_cooldown_seconds";
const IS_MAX_PER_STREAM_ENABLED_KEY: &str = "is_max_per_stream_enabled";
const IS_MAX_PER_USER_PER_STREAM_ENABLED_KEY: &str = "is_max_per_user_per_stream_enabled";
const IS_GLOBAL_COOLDOWN_ENABLED_KEY: &str = "is_global_cooldown_enabled";

pub(crate) fn is_valid_hex_color(s: &str) -> bool {
    if s.len() != HEX_COLOR_LEN {
        return false;
    }
    let mut chars = s.chars();
    chars.next() == Some(HEX_COLOR_PREFIX) && chars.all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RewardBody(serde_json::Map<String, serde_json::Value>);

impl RewardBody {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn set(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.0.insert(key.to_owned(), value.into());
        self
    }

    pub(crate) fn title(self, title: impl Into<String>) -> Self {
        self.set(TITLE_KEY, title.into())
    }

    pub(crate) fn cost(self, cost: i64) -> Self {
        self.set(COST_KEY, cost)
    }

    pub(crate) fn prompt(self, prompt: impl Into<String>) -> Self {
        self.set(PROMPT_KEY, prompt.into())
    }

    pub(crate) fn background_color(self, hex: impl Into<String>) -> Self {
        self.set(BACKGROUND_COLOR_KEY, hex.into())
    }

    pub(crate) fn enabled(self, on: bool) -> Self {
        self.set(IS_ENABLED_KEY, on)
    }

    pub(crate) fn paused(self, on: bool) -> Self {
        self.set(IS_PAUSED_KEY, on)
    }

    pub(crate) fn user_input_required(self, on: bool) -> Self {
        self.set(IS_USER_INPUT_REQUIRED_KEY, on)
    }

    pub(crate) fn skip_request_queue(self, on: bool) -> Self {
        self.set(SKIP_REQUEST_QUEUE_KEY, on)
    }

    pub(crate) fn max_per_stream(self, limit: i64) -> Self {
        self.limit(IS_MAX_PER_STREAM_ENABLED_KEY, MAX_PER_STREAM_KEY, limit)
    }

    pub(crate) fn max_per_user_per_stream(self, limit: i64) -> Self {
        self.limit(
            IS_MAX_PER_USER_PER_STREAM_ENABLED_KEY,
            MAX_PER_USER_PER_STREAM_KEY,
            limit,
        )
    }

    pub(crate) fn global_cooldown_seconds(self, seconds: i64) -> Self {
        self.limit(
            IS_GLOBAL_COOLDOWN_ENABLED_KEY,
            GLOBAL_COOLDOWN_SECONDS_KEY,
            seconds,
        )
    }

    fn limit(self, switch_key: &str, value_key: &str, value: i64) -> Self {
        if value > 0 {
            self.set(switch_key, true).set(value_key, value)
        } else {
            self.set(switch_key, false)
        }
    }

    fn into_value(self) -> serde_json::Value {
        serde_json::Value::Object(self.0)
    }
}

pub(crate) fn list_request(
    broadcaster_id: impl Into<String>,
    only_manageable: bool,
) -> HelixRequest {
    let request = HelixRequest::new(HelixMethod::Get, CUSTOM_REWARDS_PATH)
        .query(BROADCASTER_ID_PARAM, broadcaster_id);
    if only_manageable {
        request.query(ONLY_MANAGEABLE_PARAM, true.to_string())
    } else {
        request
    }
}

pub(crate) fn unfulfilled_redemptions_request(
    broadcaster_id: impl Into<String>,
    reward_id: impl Into<String>,
) -> HelixRequest {
    HelixRequest::new(HelixMethod::Get, CUSTOM_REWARD_REDEMPTIONS_PATH)
        .query(BROADCASTER_ID_PARAM, broadcaster_id)
        .query(REDEEMED_REWARD_ID_PARAM, reward_id)
        .query(STATUS_PARAM, STATUS_UNFULFILLED)
        .query(FIRST_PARAM, REDEMPTIONS_PAGE_SIZE.to_string())
}

pub(crate) fn create_request(broadcaster_id: impl Into<String>, body: RewardBody) -> HelixRequest {
    HelixRequest::new(HelixMethod::Post, CUSTOM_REWARDS_PATH)
        .query(BROADCASTER_ID_PARAM, broadcaster_id)
        .body(body.into_value())
}

pub(crate) fn update_request(
    broadcaster_id: impl Into<String>,
    reward_id: impl Into<String>,
    body: RewardBody,
) -> HelixRequest {
    HelixRequest::new(HelixMethod::Patch, CUSTOM_REWARDS_PATH)
        .query(BROADCASTER_ID_PARAM, broadcaster_id)
        .query(REWARD_ID_PARAM, reward_id)
        .body(body.into_value())
}

pub(crate) fn delete_request(
    broadcaster_id: impl Into<String>,
    reward_id: impl Into<String>,
) -> HelixRequest {
    HelixRequest::new(HelixMethod::Delete, CUSTOM_REWARDS_PATH)
        .query(BROADCASTER_ID_PARAM, broadcaster_id)
        .query(REWARD_ID_PARAM, reward_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfulfilled_redemptions_are_read_for_one_reward_a_full_page_at_a_time() {
        let request = unfulfilled_redemptions_request("4242", "reward-7");

        let mut query: Vec<(&str, &str)> = request
            .query
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        query.sort_unstable();
        assert_eq!(
            (request.method, request.path, query),
            (
                HelixMethod::Get,
                "/helix/channel_points/custom_rewards/redemptions",
                vec![
                    ("broadcaster_id", "4242"),
                    ("first", "50"),
                    ("reward_id", "reward-7"),
                    ("status", "UNFULFILLED"),
                ],
            )
        );
    }
}
