use std::collections::{BTreeMap, HashSet};

use async_trait::async_trait;
use forge_platform_core::{
    BuiltinCollections, CollectionFailure, CollectionField, CollectionId, CollectionItem,
    CollectionItemAccess, CollectionItemId, CollectionMetadata, CollectionOutcome,
    CollectionRevisions, CollectionToggle, QuickActionFieldValue, SectionIcon,
};
use reqwest::StatusCode;

use crate::builtin::{TwitchIntegrationBundle, int_field, text_field_placeholder, toggle_field};
use crate::custom_rewards::{
    BACKGROUND_COLOR_KEY, COST_KEY, GLOBAL_COOLDOWN_SECONDS_KEY, HEX_COLOR_LEN, IS_ENABLED_KEY,
    IS_PAUSED_KEY, IS_USER_INPUT_REQUIRED_KEY, MAX_CUSTOM_REWARDS, MAX_PER_STREAM_KEY,
    MAX_PER_USER_PER_STREAM_KEY, MAX_PROMPT_CHARS, MAX_TITLE_CHARS, MIN_COST, PROMPT_KEY,
    RewardBody, TITLE_KEY, create_request, delete_request, first_row, is_valid_hex_color,
    list_request, update_request,
};
use crate::helix::{HelixError, HelixRequest};
use crate::sub_actions::identity::BroadcasterTier;

pub(crate) const REWARDS_COLLECTION: &str = "rewards";

const ENABLED_TOGGLE: &str = "enabled";
const PAUSED_TOGGLE: &str = "paused";
const DEFAULT_REWARD_COST: i64 = 100;
const UNLIMITED: i64 = 0;

const ID_KEY: &str = "id";
const MAX_PER_STREAM_SETTING_KEY: &str = "max_per_stream_setting";
const MAX_PER_USER_PER_STREAM_SETTING_KEY: &str = "max_per_user_per_stream_setting";
const GLOBAL_COOLDOWN_SETTING_KEY: &str = "global_cooldown_setting";
const SETTING_ENABLED_KEY: &str = "is_enabled";
const ERROR_MESSAGE_KEY: &str = "message";

const NOT_OWNED_REASON: &str = "Created outside forge - edit it in the Twitch dashboard";

const CAPACITY_NEEDLES: &[&str] = &["too many", "maximum number", "max number", "reward limit"];
const AUTOMOD_NEEDLE: &str = "automod";
const DUPLICATE_NEEDLE: &str = "duplicate";

const FIELD_NEEDLES: &[(&str, &str, &str)] = &[
    (
        "max per user",
        MAX_PER_USER_PER_STREAM_KEY,
        "Twitch rejected the per-viewer limit",
    ),
    (
        "max per stream",
        MAX_PER_STREAM_KEY,
        "Twitch rejected the per-stream limit",
    ),
    (
        "cooldown",
        GLOBAL_COOLDOWN_SECONDS_KEY,
        "Twitch rejected the cooldown",
    ),
    (
        "color",
        BACKGROUND_COLOR_KEY,
        "Twitch rejected the background color",
    ),
    ("prompt", PROMPT_KEY, "Twitch rejected the prompt"),
    ("cost", COST_KEY, "Twitch rejected the cost"),
    ("title", TITLE_KEY, "Twitch rejected the title"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RewardCall {
    List,
    Create,
    Modify,
}

struct RewardDraft {
    title: String,
    cost: i64,
    prompt: String,
    user_input_required: bool,
    background_color: String,
    max_per_stream: i64,
    max_per_user_per_stream: i64,
    global_cooldown_seconds: i64,
}

impl RewardDraft {
    fn from_values(values: &BTreeMap<String, QuickActionFieldValue>) -> CollectionOutcome<Self> {
        let title = text_value(values, TITLE_KEY).trim().to_owned();
        if title.is_empty() {
            return Err(invalid(TITLE_KEY, "Title is required".to_owned()));
        }
        if title.chars().count() > MAX_TITLE_CHARS {
            return Err(invalid(
                TITLE_KEY,
                format!("Title must be at most {MAX_TITLE_CHARS} characters"),
            ));
        }

        let cost = int_value(values, COST_KEY).unwrap_or(DEFAULT_REWARD_COST);
        if cost < MIN_COST {
            return Err(invalid(
                COST_KEY,
                format!("Cost must be at least {MIN_COST}"),
            ));
        }

        let prompt = text_value(values, PROMPT_KEY);
        if prompt.chars().count() > MAX_PROMPT_CHARS {
            return Err(invalid(
                PROMPT_KEY,
                format!("Prompt must be at most {MAX_PROMPT_CHARS} characters"),
            ));
        }

        let background_color = text_value(values, BACKGROUND_COLOR_KEY).trim().to_owned();
        if !background_color.is_empty() && !is_valid_hex_color(&background_color) {
            return Err(invalid(
                BACKGROUND_COLOR_KEY,
                "Background color must be empty or #RRGGBB".to_owned(),
            ));
        }

        Ok(Self {
            title,
            cost,
            prompt,
            user_input_required: toggle_value(values, IS_USER_INPUT_REQUIRED_KEY),
            background_color,
            max_per_stream: limit_value(values, MAX_PER_STREAM_KEY)?,
            max_per_user_per_stream: limit_value(values, MAX_PER_USER_PER_STREAM_KEY)?,
            global_cooldown_seconds: limit_value(values, GLOBAL_COOLDOWN_SECONDS_KEY)?,
        })
    }

    fn into_body(self, call: RewardCall) -> RewardBody {
        let mut body = RewardBody::new()
            .title(self.title)
            .cost(self.cost)
            .user_input_required(self.user_input_required)
            .max_per_stream(self.max_per_stream)
            .max_per_user_per_stream(self.max_per_user_per_stream)
            .global_cooldown_seconds(self.global_cooldown_seconds);
        if call != RewardCall::Create || !self.prompt.is_empty() {
            body = body.prompt(self.prompt);
        }
        if !self.background_color.is_empty() {
            body = body.background_color(self.background_color);
        }
        body
    }
}

fn invalid(field: &str, message: String) -> CollectionFailure {
    CollectionFailure::InvalidInput {
        field: Some(field.to_owned()),
        message,
    }
}

fn text_value(values: &BTreeMap<String, QuickActionFieldValue>, key: &str) -> String {
    match values.get(key) {
        Some(QuickActionFieldValue::Text(text)) => text.clone(),
        _ => String::new(),
    }
}

fn int_value(values: &BTreeMap<String, QuickActionFieldValue>, key: &str) -> Option<i64> {
    match values.get(key) {
        Some(QuickActionFieldValue::Int(n)) => Some(*n),
        _ => None,
    }
}

fn toggle_value(values: &BTreeMap<String, QuickActionFieldValue>, key: &str) -> bool {
    matches!(values.get(key), Some(QuickActionFieldValue::Toggle(true)))
}

fn limit_value(
    values: &BTreeMap<String, QuickActionFieldValue>,
    key: &str,
) -> CollectionOutcome<i64> {
    let value = int_value(values, key).unwrap_or(UNLIMITED);
    if value < UNLIMITED {
        return Err(invalid(key, "Must be 0 or more".to_owned()));
    }
    Ok(value)
}

fn rows(response: &serde_json::Value) -> impl Iterator<Item = &serde_json::Value> {
    response["data"].as_array().into_iter().flatten()
}

fn row_id(row: &serde_json::Value) -> Option<&str> {
    row[ID_KEY].as_str()
}

fn row_text(row: &serde_json::Value, key: &str) -> QuickActionFieldValue {
    QuickActionFieldValue::Text(row[key].as_str().unwrap_or_default().to_owned())
}

fn row_flag(row: &serde_json::Value, key: &str) -> bool {
    row[key].as_bool().unwrap_or(false)
}

fn row_limit(row: &serde_json::Value, setting_key: &str, value_key: &str) -> QuickActionFieldValue {
    let setting = &row[setting_key];
    let limit = if row_flag(setting, SETTING_ENABLED_KEY) {
        setting[value_key].as_i64().unwrap_or(UNLIMITED)
    } else {
        UNLIMITED
    };
    QuickActionFieldValue::Int(limit)
}

fn item_from_row(row: &serde_json::Value, manageable: bool) -> Option<CollectionItem> {
    let id = row_id(row)?;
    let title = row[TITLE_KEY].as_str().unwrap_or_default().to_owned();
    let values = BTreeMap::from([
        (TITLE_KEY.to_owned(), row_text(row, TITLE_KEY)),
        (
            COST_KEY.to_owned(),
            QuickActionFieldValue::Int(row[COST_KEY].as_i64().unwrap_or(MIN_COST)),
        ),
        (PROMPT_KEY.to_owned(), row_text(row, PROMPT_KEY)),
        (
            IS_USER_INPUT_REQUIRED_KEY.to_owned(),
            QuickActionFieldValue::Toggle(row_flag(row, IS_USER_INPUT_REQUIRED_KEY)),
        ),
        (
            BACKGROUND_COLOR_KEY.to_owned(),
            row_text(row, BACKGROUND_COLOR_KEY),
        ),
        (
            MAX_PER_STREAM_KEY.to_owned(),
            row_limit(row, MAX_PER_STREAM_SETTING_KEY, MAX_PER_STREAM_KEY),
        ),
        (
            MAX_PER_USER_PER_STREAM_KEY.to_owned(),
            row_limit(
                row,
                MAX_PER_USER_PER_STREAM_SETTING_KEY,
                MAX_PER_USER_PER_STREAM_KEY,
            ),
        ),
        (
            GLOBAL_COOLDOWN_SECONDS_KEY.to_owned(),
            row_limit(
                row,
                GLOBAL_COOLDOWN_SETTING_KEY,
                GLOBAL_COOLDOWN_SECONDS_KEY,
            ),
        ),
    ]);
    let toggles = BTreeMap::from([
        (ENABLED_TOGGLE.to_owned(), row_flag(row, IS_ENABLED_KEY)),
        (PAUSED_TOGGLE.to_owned(), row_flag(row, IS_PAUSED_KEY)),
    ]);
    let access = if manageable {
        CollectionItemAccess::Manageable
    } else {
        CollectionItemAccess::ReadOnly {
            reason: NOT_OWNED_REASON.to_owned(),
        }
    };
    Some(CollectionItem {
        id: CollectionItemId::new(id),
        title,
        values,
        toggles,
        access,
    })
}

fn normalized_error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|parsed| parsed[ERROR_MESSAGE_KEY].as_str().map(str::to_owned))
        .unwrap_or_default()
        .to_lowercase()
        .replace('_', " ")
}

fn mentions_capacity(message: &str) -> bool {
    CAPACITY_NEEDLES
        .iter()
        .any(|needle| message.contains(needle))
}

fn bad_request_failure(body: &str) -> CollectionFailure {
    let message = normalized_error_message(body);
    if mentions_capacity(&message) {
        return CollectionFailure::CapacityReached;
    }
    if message.contains(DUPLICATE_NEEDLE) {
        return invalid(
            TITLE_KEY,
            "A reward with this title already exists".to_owned(),
        );
    }
    if message.contains(AUTOMOD_NEEDLE) {
        return CollectionFailure::InvalidInput {
            field: None,
            message: "Twitch AutoMod rejected the reward text".to_owned(),
        };
    }
    FIELD_NEEDLES
        .iter()
        .find(|(needle, _, _)| message.contains(needle))
        .map(|(_, field, safe_message)| invalid(field, (*safe_message).to_owned()))
        .unwrap_or_else(|| CollectionFailure::InvalidInput {
            field: None,
            message: "Twitch rejected the reward settings".to_owned(),
        })
}

fn forbidden_failure(body: &str, call: RewardCall, tier: BroadcasterTier) -> CollectionFailure {
    match call {
        RewardCall::List => CollectionFailure::NotEligible,
        RewardCall::Create if mentions_capacity(&normalized_error_message(body)) => {
            CollectionFailure::CapacityReached
        }
        RewardCall::Create => CollectionFailure::NotEligible,
        RewardCall::Modify if tier == BroadcasterTier::Standard => CollectionFailure::NotEligible,
        RewardCall::Modify => CollectionFailure::NotOwned,
    }
}

fn unknown_collection() -> CollectionFailure {
    CollectionFailure::InvalidInput {
        field: None,
        message: "Unknown collection".to_owned(),
    }
}

fn unknown_toggle(toggle: &str) -> CollectionFailure {
    CollectionFailure::InvalidInput {
        field: Some(toggle.to_owned()),
        message: "Unknown reward toggle".to_owned(),
    }
}

fn reward_gone() -> CollectionFailure {
    CollectionFailure::InvalidInput {
        field: None,
        message: "This reward no longer exists".to_owned(),
    }
}

fn rewards_metadata() -> CollectionMetadata {
    CollectionMetadata {
        id: CollectionId::new(REWARDS_COLLECTION),
        label: "Channel point rewards".to_owned(),
        icon: SectionIcon::new("diamond"),
        capacity: Some(MAX_CUSTOM_REWARDS),
        fields: vec![
            CollectionField {
                field: text_field_placeholder(TITLE_KEY, "Title", "", "Hydrate!").required(),
                max_chars: Some(MAX_TITLE_CHARS),
            },
            CollectionField {
                field: int_field(
                    COST_KEY,
                    "Cost (Channel Points)",
                    DEFAULT_REWARD_COST,
                    MIN_COST,
                    i64::MAX,
                )
                .required(),
                max_chars: None,
            },
            CollectionField {
                field: text_field_placeholder(PROMPT_KEY, "Prompt", "", "optional"),
                max_chars: Some(MAX_PROMPT_CHARS),
            },
            CollectionField {
                field: toggle_field(IS_USER_INPUT_REQUIRED_KEY, "Require viewer input", false),
                max_chars: None,
            },
            CollectionField {
                field: text_field_placeholder(
                    BACKGROUND_COLOR_KEY,
                    "Background color (#RRGGBB)",
                    "",
                    "#9147FF",
                ),
                max_chars: Some(HEX_COLOR_LEN),
            },
            CollectionField {
                field: int_field(
                    MAX_PER_STREAM_KEY,
                    "Max per stream (0 = unlimited)",
                    UNLIMITED,
                    UNLIMITED,
                    i64::MAX,
                ),
                max_chars: None,
            },
            CollectionField {
                field: int_field(
                    MAX_PER_USER_PER_STREAM_KEY,
                    "Max per viewer per stream (0 = unlimited)",
                    UNLIMITED,
                    UNLIMITED,
                    i64::MAX,
                ),
                max_chars: None,
            },
            CollectionField {
                field: int_field(
                    GLOBAL_COOLDOWN_SECONDS_KEY,
                    "Global cooldown seconds (0 = off)",
                    UNLIMITED,
                    UNLIMITED,
                    i64::MAX,
                ),
                max_chars: None,
            },
        ],
        toggles: vec![
            CollectionToggle {
                key: ENABLED_TOGGLE.to_owned(),
                label: "Enabled".to_owned(),
            },
            CollectionToggle {
                key: PAUSED_TOGGLE.to_owned(),
                label: "Paused".to_owned(),
            },
        ],
    }
}

impl TwitchIntegrationBundle {
    fn rewards_broadcaster(&self, collection: &CollectionId) -> CollectionOutcome<String> {
        if collection.as_str() != REWARDS_COLLECTION {
            return Err(unknown_collection());
        }
        let broadcaster_id = self.broadcaster_id();
        if broadcaster_id.is_empty() {
            return Err(CollectionFailure::NotConnected);
        }
        Ok(broadcaster_id.to_owned())
    }

    async fn reward_call(
        &self,
        request: HelixRequest,
        call: RewardCall,
    ) -> CollectionOutcome<serde_json::Value> {
        self.helix()
            .execute(request)
            .await
            .map_err(|e| self.reward_failure(e, call))
    }

    fn reward_failure(&self, error: HelixError, call: RewardCall) -> CollectionFailure {
        match error {
            HelixError::RateLimited => CollectionFailure::RateLimited,
            HelixError::ReauthRequired => CollectionFailure::Unauthorized,
            HelixError::Credentials(_) => CollectionFailure::NotConnected,
            HelixError::Transport(_) => CollectionFailure::Transport,
            HelixError::Http { status, body } => match StatusCode::from_u16(status) {
                Ok(StatusCode::BAD_REQUEST) => bad_request_failure(&body),
                Ok(StatusCode::FORBIDDEN) => forbidden_failure(&body, call, self.tier()),
                Ok(StatusCode::NOT_FOUND) if call == RewardCall::Modify => {
                    self.lifecycle().rewards_changed();
                    reward_gone()
                }
                _ => CollectionFailure::Transport,
            },
        }
    }

    async fn write_reward(
        &self,
        request: HelixRequest,
        call: RewardCall,
    ) -> CollectionOutcome<CollectionItem> {
        let response = self.reward_call(request, call).await?;
        first_row(&response)
            .and_then(|row| item_from_row(row, true))
            .ok_or(CollectionFailure::Transport)
    }
}

#[async_trait]
impl BuiltinCollections for TwitchIntegrationBundle {
    fn collections(&self) -> Vec<CollectionMetadata> {
        vec![rewards_metadata()]
    }

    fn revisions(&self) -> CollectionRevisions {
        self.lifecycle().reward_revisions()
    }

    async fn list(&self, collection: &CollectionId) -> CollectionOutcome<Vec<CollectionItem>> {
        let broadcaster_id = self.rewards_broadcaster(collection)?;
        let all = self
            .reward_call(
                list_request(broadcaster_id.clone(), false),
                RewardCall::List,
            )
            .await?;
        let manageable = self
            .reward_call(list_request(broadcaster_id, true), RewardCall::List)
            .await?;
        let owned: HashSet<&str> = rows(&manageable).filter_map(row_id).collect();
        Ok(rows(&all)
            .filter_map(|row| {
                let is_owned = row_id(row).is_some_and(|id| owned.contains(id));
                item_from_row(row, is_owned)
            })
            .collect())
    }

    async fn create(
        &self,
        collection: &CollectionId,
        values: &BTreeMap<String, QuickActionFieldValue>,
    ) -> CollectionOutcome<CollectionItem> {
        let broadcaster_id = self.rewards_broadcaster(collection)?;
        let body = RewardDraft::from_values(values)?.into_body(RewardCall::Create);
        self.write_reward(create_request(broadcaster_id, body), RewardCall::Create)
            .await
    }

    async fn update(
        &self,
        collection: &CollectionId,
        item: &CollectionItemId,
        values: &BTreeMap<String, QuickActionFieldValue>,
    ) -> CollectionOutcome<CollectionItem> {
        let broadcaster_id = self.rewards_broadcaster(collection)?;
        let body = RewardDraft::from_values(values)?.into_body(RewardCall::Modify);
        self.write_reward(
            update_request(broadcaster_id, item.as_str(), body),
            RewardCall::Modify,
        )
        .await
    }

    async fn delete(
        &self,
        collection: &CollectionId,
        item: &CollectionItemId,
    ) -> CollectionOutcome<()> {
        let broadcaster_id = self.rewards_broadcaster(collection)?;
        self.reward_call(
            delete_request(broadcaster_id, item.as_str()),
            RewardCall::Modify,
        )
        .await
        .map(|_| ())
    }

    async fn set_toggle(
        &self,
        collection: &CollectionId,
        item: &CollectionItemId,
        toggle: &str,
        on: bool,
    ) -> CollectionOutcome<CollectionItem> {
        let broadcaster_id = self.rewards_broadcaster(collection)?;
        let body = match toggle {
            ENABLED_TOGGLE => RewardBody::new().enabled(on),
            PAUSED_TOGGLE => RewardBody::new().paused(on),
            other => return Err(unknown_toggle(other)),
        };
        self.write_reward(
            update_request(broadcaster_id, item.as_str(), body),
            RewardCall::Modify,
        )
        .await
    }
}
