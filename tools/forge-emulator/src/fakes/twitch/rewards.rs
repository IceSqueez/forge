use axum::http::StatusCode;
use serde_json::{Map, Value, json};

use super::config::FakeTwitchConfig;
use super::ids;

pub const MAX_CUSTOM_REWARDS: usize = 50;
const MAX_TITLE_CHARS: usize = 45;
const MAX_PROMPT_CHARS: usize = 200;
const MIN_COST: i64 = 1;
const HEX_COLOR_LEN: usize = 7;
const HEX_COLOR_PREFIX: char = '#';
const DEFAULT_BACKGROUND: &str = "#9147FF";
const DASHBOARD_CLIENT_ID: &str = "twitch-dashboard";

pub(crate) const REWARD_ADD: &str = "channel.channel_points_custom_reward.add";
pub(crate) const REWARD_UPDATE: &str = "channel.channel_points_custom_reward.update";
pub(crate) const REWARD_REMOVE: &str = "channel.channel_points_custom_reward.remove";

const DUPLICATE_ON_CREATE: &str = "CREATE_CUSTOM_REWARD_DUPLICATE_REWARD";
const DUPLICATE_ON_UPDATE: &str = "UPDATE_CUSTOM_REWARD_DUPLICATE_REWARD";
const TOO_MANY_REWARDS: &str = "CREATE_CUSTOM_REWARD_TOO_MANY_REWARDS";
const NOT_OWNED: &str =
    "The ID in the Client-Id header must match the client ID used to create the custom reward.";
const WRONG_BROADCASTER: &str =
    "The ID in broadcaster_id must match the user ID in the user access token.";
const MISSING_ID: &str = "Missing required parameter id";
const NOT_FOUND: &str = "Custom reward not found";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeReward {
    pub id: String,
    pub owner_client_id: String,
    pub title: String,
    pub cost: i64,
    pub prompt: String,
    pub background_color: String,
    pub is_enabled: bool,
    pub is_paused: bool,
    pub is_user_input_required: bool,
    pub skip_request_queue: bool,
    pub max_per_stream: Option<i64>,
    pub max_per_user_per_stream: Option<i64>,
    pub global_cooldown_seconds: Option<i64>,
}

impl FakeReward {
    fn new(owner_client_id: &str, title: &str) -> Self {
        Self {
            id: ids::uuid_like(),
            owner_client_id: owner_client_id.to_owned(),
            title: title.to_owned(),
            cost: MIN_COST,
            prompt: String::new(),
            background_color: DEFAULT_BACKGROUND.to_owned(),
            is_enabled: true,
            is_paused: false,
            is_user_input_required: false,
            skip_request_queue: false,
            max_per_stream: None,
            max_per_user_per_stream: None,
            global_cooldown_seconds: None,
        }
    }

    fn helix_row(&self, config: &FakeTwitchConfig) -> Value {
        json!({
            "broadcaster_id": config.broadcaster_user_id,
            "broadcaster_login": config.broadcaster_login,
            "broadcaster_name": config.broadcaster_login,
            "id": self.id,
            "title": self.title,
            "prompt": self.prompt,
            "cost": self.cost,
            "image": null,
            "default_image": default_image(),
            "background_color": self.background_color,
            "is_enabled": self.is_enabled,
            "is_user_input_required": self.is_user_input_required,
            "max_per_stream_setting": {
                "is_enabled": self.max_per_stream.is_some(),
                "max_per_stream": self.max_per_stream.unwrap_or(0),
            },
            "max_per_user_per_stream_setting": {
                "is_enabled": self.max_per_user_per_stream.is_some(),
                "max_per_user_per_stream": self.max_per_user_per_stream.unwrap_or(0),
            },
            "global_cooldown_setting": {
                "is_enabled": self.global_cooldown_seconds.is_some(),
                "global_cooldown_seconds": self.global_cooldown_seconds.unwrap_or(0),
            },
            "is_paused": self.is_paused,
            "is_in_stock": true,
            "should_redemptions_skip_request_queue": self.skip_request_queue,
            "redemptions_redeemed_current_stream": null,
            "cooldown_expires_at": null,
        })
    }

    fn eventsub_event(&self, config: &FakeTwitchConfig) -> Value {
        json!({
            "id": self.id,
            "broadcaster_user_id": config.broadcaster_user_id,
            "broadcaster_user_login": config.broadcaster_login,
            "broadcaster_user_name": config.broadcaster_login,
            "is_enabled": self.is_enabled,
            "is_paused": self.is_paused,
            "is_in_stock": true,
            "title": self.title,
            "cost": self.cost,
            "prompt": self.prompt,
            "is_user_input_required": self.is_user_input_required,
            "should_redemptions_skip_request_queue": self.skip_request_queue,
            "cooldown_expires_at": null,
            "redemptions_redeemed_current_stream": null,
            "max_per_stream": {
                "is_enabled": self.max_per_stream.is_some(),
                "value": self.max_per_stream.unwrap_or(0),
            },
            "max_per_user_per_stream": {
                "is_enabled": self.max_per_user_per_stream.is_some(),
                "value": self.max_per_user_per_stream.unwrap_or(0),
            },
            "global_cooldown": {
                "is_enabled": self.global_cooldown_seconds.is_some(),
                "seconds": self.global_cooldown_seconds.unwrap_or(0),
            },
            "background_color": self.background_color,
            "image": null,
            "default_image": default_image(),
        })
    }

    fn apply(&mut self, body: &Map<String, Value>) -> Result<(), &'static str> {
        if let Some(title) = body.get("title") {
            let title = title.as_str().ok_or("title must be a string")?.trim();
            if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
                return Err("title must be 1 to 45 characters");
            }
            self.title = title.to_owned();
        }
        if let Some(cost) = body.get("cost") {
            let cost = cost.as_i64().ok_or("cost must be an integer")?;
            if cost < MIN_COST {
                return Err("cost must be at least 1");
            }
            self.cost = cost;
        }
        if let Some(prompt) = body.get("prompt") {
            let prompt = prompt.as_str().ok_or("prompt must be a string")?;
            if prompt.chars().count() > MAX_PROMPT_CHARS {
                return Err("prompt must be at most 200 characters");
            }
            self.prompt = prompt.to_owned();
        }
        if let Some(color) = body.get("background_color") {
            self.background_color = color
                .as_str()
                .filter(|color| is_hex_color(color))
                .ok_or("background_color must be #RRGGBB")?
                .to_owned();
        }
        apply_flag(body, "is_enabled", &mut self.is_enabled)?;
        apply_flag(body, "is_paused", &mut self.is_paused)?;
        apply_flag(
            body,
            "is_user_input_required",
            &mut self.is_user_input_required,
        )?;
        apply_flag(
            body,
            "should_redemptions_skip_request_queue",
            &mut self.skip_request_queue,
        )?;
        apply_limit(
            body,
            "is_max_per_stream_enabled",
            "max_per_stream",
            &mut self.max_per_stream,
        )?;
        apply_limit(
            body,
            "is_max_per_user_per_stream_enabled",
            "max_per_user_per_stream",
            &mut self.max_per_user_per_stream,
        )?;
        apply_limit(
            body,
            "is_global_cooldown_enabled",
            "global_cooldown_seconds",
            &mut self.global_cooldown_seconds,
        )
    }
}

fn default_image() -> Value {
    json!({
        "url_1x": "https://static-cdn.jtvnw.net/custom-reward-images/default-1.png",
        "url_2x": "https://static-cdn.jtvnw.net/custom-reward-images/default-2.png",
        "url_4x": "https://static-cdn.jtvnw.net/custom-reward-images/default-4.png",
    })
}

fn is_hex_color(color: &str) -> bool {
    color.len() == HEX_COLOR_LEN
        && color.starts_with(HEX_COLOR_PREFIX)
        && color.chars().skip(1).all(|c| c.is_ascii_hexdigit())
}

fn apply_flag(body: &Map<String, Value>, key: &str, slot: &mut bool) -> Result<(), &'static str> {
    if let Some(value) = body.get(key) {
        *slot = value.as_bool().ok_or("flags must be booleans")?;
    }
    Ok(())
}

fn apply_limit(
    body: &Map<String, Value>,
    switch_key: &str,
    value_key: &str,
    slot: &mut Option<i64>,
) -> Result<(), &'static str> {
    match body.get(switch_key).map(Value::as_bool) {
        None => Ok(()),
        Some(None) => Err("limit switches must be booleans"),
        Some(Some(false)) => {
            *slot = None;
            Ok(())
        }
        Some(Some(true)) => {
            let value = body
                .get(value_key)
                .and_then(Value::as_i64)
                .filter(|value| *value >= MIN_COST)
                .ok_or("an enabled limit needs a positive value")?;
            *slot = Some(value);
            Ok(())
        }
    }
}

pub(crate) struct RewardAnswer {
    pub(crate) status: StatusCode,
    pub(crate) body: Value,
    pub(crate) notification: Option<(&'static str, Value)>,
}

impl RewardAnswer {
    fn ok(body: Value) -> Self {
        Self {
            status: StatusCode::OK,
            body,
            notification: None,
        }
    }

    fn refused(status: StatusCode, message: &str) -> Self {
        Self {
            status,
            body: json!({
                "error": status.canonical_reason().unwrap_or_default(),
                "status": status.as_u16(),
                "message": message,
            }),
            notification: None,
        }
    }

    fn notifying(mut self, kind: &'static str, event: Value) -> Self {
        self.notification = Some((kind, event));
        self
    }
}

#[derive(Default)]
pub(crate) struct RewardStore {
    rewards: Vec<FakeReward>,
}

fn param<'a>(query: &'a [(String, String)], key: &str) -> Option<&'a str> {
    query
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

impl RewardStore {
    pub(crate) fn snapshot(&self) -> Vec<FakeReward> {
        self.rewards.clone()
    }

    pub(crate) fn seed_dashboard_reward(&mut self, title: &str) -> FakeReward {
        let reward = FakeReward::new(DASHBOARD_CLIENT_ID, title);
        self.rewards.push(reward.clone());
        reward
    }

    fn title_taken(&self, title: &str, except: Option<&str>) -> bool {
        self.rewards.iter().any(|reward| {
            Some(reward.id.as_str()) != except && reward.title.eq_ignore_ascii_case(title.trim())
        })
    }

    fn broadcaster_refusal(
        config: &FakeTwitchConfig,
        query: &[(String, String)],
    ) -> Option<RewardAnswer> {
        (param(query, "broadcaster_id") != Some(config.broadcaster_user_id.as_str()))
            .then(|| RewardAnswer::refused(StatusCode::FORBIDDEN, WRONG_BROADCASTER))
    }

    pub(crate) fn list(
        &self,
        config: &FakeTwitchConfig,
        query: &[(String, String)],
    ) -> RewardAnswer {
        if let Some(refusal) = Self::broadcaster_refusal(config, query) {
            return refusal;
        }
        let only_manageable = param(query, "only_manageable_rewards") == Some("true");
        let wanted_id = param(query, "id");
        let data: Vec<Value> = self
            .rewards
            .iter()
            .filter(|reward| !only_manageable || reward.owner_client_id == config.client_id)
            .filter(|reward| wanted_id.is_none_or(|id| reward.id == id))
            .map(|reward| reward.helix_row(config))
            .collect();
        RewardAnswer::ok(json!({ "data": data }))
    }

    pub(crate) fn create(
        &mut self,
        config: &FakeTwitchConfig,
        query: &[(String, String)],
        body: Option<&Value>,
    ) -> RewardAnswer {
        if let Some(refusal) = Self::broadcaster_refusal(config, query) {
            return refusal;
        }
        let Some(fields) = body.and_then(Value::as_object) else {
            return RewardAnswer::refused(StatusCode::BAD_REQUEST, "request body is missing");
        };
        let Some(title) = fields.get("title").and_then(Value::as_str) else {
            return RewardAnswer::refused(
                StatusCode::BAD_REQUEST,
                "Missing required parameter title",
            );
        };
        if fields.get("cost").and_then(Value::as_i64).is_none() {
            return RewardAnswer::refused(
                StatusCode::BAD_REQUEST,
                "Missing required parameter cost",
            );
        }
        if self.rewards.len() >= MAX_CUSTOM_REWARDS {
            return RewardAnswer::refused(StatusCode::BAD_REQUEST, TOO_MANY_REWARDS);
        }
        if self.title_taken(title, None) {
            return RewardAnswer::refused(StatusCode::BAD_REQUEST, DUPLICATE_ON_CREATE);
        }
        let mut reward = FakeReward::new(&config.client_id, title);
        if let Err(reason) = reward.apply(fields) {
            return RewardAnswer::refused(StatusCode::BAD_REQUEST, reason);
        }
        self.rewards.push(reward.clone());
        RewardAnswer::ok(json!({ "data": [reward.helix_row(config)] }))
            .notifying(REWARD_ADD, reward.eventsub_event(config))
    }

    fn owned_index(
        &self,
        config: &FakeTwitchConfig,
        query: &[(String, String)],
    ) -> Result<usize, RewardAnswer> {
        if let Some(refusal) = Self::broadcaster_refusal(config, query) {
            return Err(refusal);
        }
        let id = param(query, "id")
            .ok_or_else(|| RewardAnswer::refused(StatusCode::BAD_REQUEST, MISSING_ID))?;
        let index = self
            .rewards
            .iter()
            .position(|reward| reward.id == id)
            .ok_or_else(|| RewardAnswer::refused(StatusCode::NOT_FOUND, NOT_FOUND))?;
        if self.rewards[index].owner_client_id != config.client_id {
            return Err(RewardAnswer::refused(StatusCode::FORBIDDEN, NOT_OWNED));
        }
        Ok(index)
    }

    pub(crate) fn update(
        &mut self,
        config: &FakeTwitchConfig,
        query: &[(String, String)],
        body: Option<&Value>,
    ) -> RewardAnswer {
        let index = match self.owned_index(config, query) {
            Ok(index) => index,
            Err(refusal) => return refusal,
        };
        let Some(fields) = body.and_then(Value::as_object) else {
            return RewardAnswer::refused(StatusCode::BAD_REQUEST, "request body is missing");
        };
        let id = self.rewards[index].id.clone();
        if let Some(title) = fields.get("title").and_then(Value::as_str)
            && self.title_taken(title, Some(&id))
        {
            return RewardAnswer::refused(StatusCode::BAD_REQUEST, DUPLICATE_ON_UPDATE);
        }
        let mut updated = self.rewards[index].clone();
        if let Err(reason) = updated.apply(fields) {
            return RewardAnswer::refused(StatusCode::BAD_REQUEST, reason);
        }
        self.rewards[index] = updated.clone();
        RewardAnswer::ok(json!({ "data": [updated.helix_row(config)] }))
            .notifying(REWARD_UPDATE, updated.eventsub_event(config))
    }

    pub(crate) fn delete(
        &mut self,
        config: &FakeTwitchConfig,
        query: &[(String, String)],
    ) -> RewardAnswer {
        let index = match self.owned_index(config, query) {
            Ok(index) => index,
            Err(refusal) => return refusal,
        };
        let removed = self.rewards.remove(index);
        RewardAnswer {
            status: StatusCode::NO_CONTENT,
            body: Value::Null,
            notification: None,
        }
        .notifying(REWARD_REMOVE, removed.eventsub_event(config))
    }
}
