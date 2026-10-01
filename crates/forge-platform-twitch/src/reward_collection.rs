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
pub(crate) const REWARD_OPTIONS_KEY: &str = "collections.twitch.rewards";
pub(crate) const MANAGEABLE_REWARD_OPTIONS_KEY: &str = "collections.twitch.rewards.manageable";

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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use forge_registry::SubActionRunner;
    use forge_types::{ArgStack, Variant};
    use serde_json::{Value, json};
    use tokio::sync::watch;

    use super::*;
    use crate::chat::ChatConnectionState;
    use crate::helix::{HelixMethod, HelixTransport};
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };
    use crate::sub_actions::{
        DeleteRewardRunner, DisableRewardRunner, EnableRewardRunner, PauseRewardRunner,
        ResumeRewardRunner, SelfIdentity,
    };
    use crate::subscriptions::SubscriptionTracker;

    const LEAKY_URL: &str = "https://api.twitch.tv/helix/channel_points/custom_rewards";

    fn bundle_with(
        responses: Vec<Result<Value, HelixError>>,
        tier: BroadcasterTier,
        broadcaster_id: &str,
    ) -> (Arc<MockTransport>, Arc<TwitchIntegrationBundle>) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let (_tx, rx) = watch::channel(ChatConnectionState::Connected);
        let bundle = TwitchIntegrationBundle::for_test_with_transport(
            Some("streamer".to_owned()),
            rx,
            SubscriptionTracker::default(),
            Arc::new(MockCreds::with_identity()),
            tier,
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            broadcaster_id,
        );
        (transport, bundle)
    }

    fn bundle_answering(
        responses: Vec<Result<Value, HelixError>>,
    ) -> (Arc<MockTransport>, Arc<TwitchIntegrationBundle>) {
        bundle_with(responses, BroadcasterTier::Affiliate, SELF_USER_ID)
    }

    fn rewards() -> CollectionId {
        CollectionId::new(REWARDS_COLLECTION)
    }

    fn reward_row(id: &str, title: &str) -> Value {
        json!({
            "id": id,
            "title": title,
            "cost": 250,
            "prompt": "say hi",
            "is_enabled": true,
            "is_paused": false,
            "is_user_input_required": true,
            "background_color": "#9147FF",
            "max_per_stream_setting": { "is_enabled": true, "max_per_stream": 3 },
            "max_per_user_per_stream_setting": { "is_enabled": false, "max_per_user_per_stream": 9 },
            "global_cooldown_setting": { "is_enabled": true, "global_cooldown_seconds": 60 },
        })
    }

    fn rows_of(rows: Vec<Value>) -> Result<Value, HelixError> {
        Ok(json!({ "data": rows }))
    }

    fn http(status: StatusCode, message: &str) -> Result<Value, HelixError> {
        Err(HelixError::Http {
            status: status.as_u16(),
            body: json!({ "error": "x", "status": status.as_u16(), "message": message })
                .to_string(),
        })
    }

    fn draft() -> BTreeMap<String, QuickActionFieldValue> {
        BTreeMap::from([
            (
                TITLE_KEY.to_owned(),
                QuickActionFieldValue::Text("  Hydrate!  ".to_owned()),
            ),
            (COST_KEY.to_owned(), QuickActionFieldValue::Int(500)),
            (
                PROMPT_KEY.to_owned(),
                QuickActionFieldValue::Text(String::new()),
            ),
            (
                IS_USER_INPUT_REQUIRED_KEY.to_owned(),
                QuickActionFieldValue::Toggle(true),
            ),
            (
                BACKGROUND_COLOR_KEY.to_owned(),
                QuickActionFieldValue::Text("#00FFaa".to_owned()),
            ),
            (MAX_PER_STREAM_KEY.to_owned(), QuickActionFieldValue::Int(5)),
            (
                MAX_PER_USER_PER_STREAM_KEY.to_owned(),
                QuickActionFieldValue::Int(0),
            ),
            (
                GLOBAL_COOLDOWN_SECONDS_KEY.to_owned(),
                QuickActionFieldValue::Int(30),
            ),
        ])
    }

    fn with(key: &str, value: QuickActionFieldValue) -> BTreeMap<String, QuickActionFieldValue> {
        let mut values = draft();
        values.insert(key.to_owned(), value);
        values
    }

    fn text(s: &str) -> QuickActionFieldValue {
        QuickActionFieldValue::Text(s.to_owned())
    }

    fn query(request: &HelixRequest) -> Vec<(String, String)> {
        request.query.clone()
    }

    fn pair(key: &str, value: &str) -> (String, String) {
        (key.to_owned(), value.to_owned())
    }

    #[tokio::test]
    async fn list_marks_rewards_outside_the_manageable_set_read_only_with_a_reason() {
        let (_, bundle) = bundle_answering(vec![
            rows_of(vec![
                reward_row("owned", "Hydrate"),
                reward_row("dash", "Stretch"),
            ]),
            rows_of(vec![reward_row("owned", "Hydrate")]),
        ]);

        let items = bundle.list(&rewards()).await.unwrap();

        let access: Vec<(&str, &CollectionItemAccess)> = items
            .iter()
            .map(|item| (item.id.as_str(), &item.access))
            .collect();
        assert_eq!(
            access,
            vec![
                ("owned", &CollectionItemAccess::Manageable),
                (
                    "dash",
                    &CollectionItemAccess::ReadOnly {
                        reason: NOT_OWNED_REASON.to_owned()
                    }
                ),
            ]
        );
    }

    #[tokio::test]
    async fn list_asks_for_every_reward_then_only_the_manageable_ones() {
        let (transport, bundle) = bundle_answering(vec![rows_of(vec![]), rows_of(vec![])]);

        bundle.list(&rewards()).await.unwrap();

        let all = transport.request(0);
        let manageable = transport.request(1);
        assert_eq!(
            (all.method, all.path.as_str(), query(&all)),
            (
                HelixMethod::Get,
                "/helix/channel_points/custom_rewards",
                vec![pair("broadcaster_id", SELF_USER_ID)]
            )
        );
        assert_eq!(
            query(&manageable),
            vec![
                pair("broadcaster_id", SELF_USER_ID),
                pair("only_manageable_rewards", "true"),
            ]
        );
    }

    #[tokio::test]
    async fn list_fails_when_the_manageable_lookup_fails() {
        let (_, bundle) = bundle_answering(vec![
            rows_of(vec![reward_row("owned", "Hydrate")]),
            http(StatusCode::INTERNAL_SERVER_ERROR, "boom"),
        ]);

        let outcome = bundle.list(&rewards()).await;

        assert_eq!(outcome, Err(CollectionFailure::Transport));
    }

    #[tokio::test]
    async fn list_skips_rows_without_an_id() {
        let (_, bundle) = bundle_answering(vec![
            rows_of(vec![
                json!({ "title": "ghost" }),
                reward_row("a", "Hydrate"),
            ]),
            rows_of(vec![]),
        ]);

        let items = bundle.list(&rewards()).await.unwrap();

        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["a"]);
    }

    #[tokio::test]
    async fn listed_limits_read_zero_when_their_setting_is_disabled() {
        let (_, bundle) = bundle_answering(vec![
            rows_of(vec![reward_row("a", "Hydrate")]),
            rows_of(vec![]),
        ]);

        let items = bundle.list(&rewards()).await.unwrap();

        let values = &items[0].values;
        assert_eq!(
            (
                values.get(MAX_PER_STREAM_KEY),
                values.get(MAX_PER_USER_PER_STREAM_KEY),
                values.get(GLOBAL_COOLDOWN_SECONDS_KEY),
            ),
            (
                Some(&QuickActionFieldValue::Int(3)),
                Some(&QuickActionFieldValue::Int(0)),
                Some(&QuickActionFieldValue::Int(60)),
            )
        );
    }

    #[tokio::test]
    async fn listed_toggles_carry_the_enabled_and_paused_flags() {
        let mut row = reward_row("a", "Hydrate");
        row["is_enabled"] = json!(false);
        row["is_paused"] = json!(true);
        let (_, bundle) = bundle_answering(vec![rows_of(vec![row]), rows_of(vec![])]);

        let items = bundle.list(&rewards()).await.unwrap();

        assert_eq!(
            items[0].toggles,
            BTreeMap::from([
                (ENABLED_TOGGLE.to_owned(), false),
                (PAUSED_TOGGLE.to_owned(), true),
            ])
        );
    }

    #[tokio::test]
    async fn create_posts_the_trimmed_draft_and_returns_a_manageable_item() {
        let (transport, bundle) =
            bundle_answering(vec![rows_of(vec![reward_row("new", "Hydrate!")])]);

        let item = bundle.create(&rewards(), &draft()).await.unwrap();

        let request = transport.request(0);
        assert_eq!(
            (
                request.method,
                request.path.as_str(),
                query(&request),
                request.body
            ),
            (
                HelixMethod::Post,
                "/helix/channel_points/custom_rewards",
                vec![pair("broadcaster_id", SELF_USER_ID)],
                Some(json!({
                    "title": "Hydrate!",
                    "cost": 500,
                    "is_user_input_required": true,
                    "background_color": "#00FFaa",
                    "is_max_per_stream_enabled": true,
                    "max_per_stream": 5,
                    "is_max_per_user_per_stream_enabled": false,
                    "is_global_cooldown_enabled": true,
                    "global_cooldown_seconds": 30,
                })),
            )
        );
        assert_eq!(
            (item.id.as_str(), item.access),
            ("new", CollectionItemAccess::Manageable)
        );
    }

    #[tokio::test]
    async fn an_empty_prompt_is_omitted_on_create_but_sent_on_update_to_clear_it() {
        let (create_transport, bundle) =
            bundle_answering(vec![rows_of(vec![reward_row("a", "Hydrate")])]);
        bundle.create(&rewards(), &draft()).await.unwrap();
        let (update_transport, bundle) =
            bundle_answering(vec![rows_of(vec![reward_row("a", "Hydrate")])]);
        bundle
            .update(&rewards(), &CollectionItemId::new("a"), &draft())
            .await
            .unwrap();

        let prompt_of =
            |transport: &MockTransport| transport.request(0).body.unwrap().get(PROMPT_KEY).cloned();
        assert_eq!(
            (prompt_of(&create_transport), prompt_of(&update_transport)),
            (None, Some(json!("")))
        );
    }

    #[tokio::test]
    async fn update_patches_the_reward_named_by_its_id() {
        let (transport, bundle) = bundle_answering(vec![rows_of(vec![reward_row("rw1", "x")])]);

        bundle
            .update(&rewards(), &CollectionItemId::new("rw1"), &draft())
            .await
            .unwrap();

        let request = transport.request(0);
        assert_eq!(
            (request.method, query(&request)),
            (
                HelixMethod::Patch,
                vec![pair("broadcaster_id", SELF_USER_ID), pair("id", "rw1")]
            )
        );
    }

    #[tokio::test]
    async fn delete_issues_a_bodyless_delete_for_the_reward() {
        let (transport, bundle) = bundle_answering(vec![Ok(Value::Null)]);

        bundle
            .delete(&rewards(), &CollectionItemId::new("rw1"))
            .await
            .unwrap();

        let request = transport.request(0);
        assert_eq!(
            (request.method, query(&request), request.body),
            (
                HelixMethod::Delete,
                vec![pair("broadcaster_id", SELF_USER_ID), pair("id", "rw1")],
                None
            )
        );
    }

    #[tokio::test]
    async fn set_toggle_patches_only_the_flag_it_names() {
        for (toggle, on, expected) in [
            (ENABLED_TOGGLE, true, json!({ "is_enabled": true })),
            (ENABLED_TOGGLE, false, json!({ "is_enabled": false })),
            (PAUSED_TOGGLE, true, json!({ "is_paused": true })),
            (PAUSED_TOGGLE, false, json!({ "is_paused": false })),
        ] {
            let (transport, bundle) = bundle_answering(vec![rows_of(vec![reward_row("rw1", "x")])]);

            bundle
                .set_toggle(&rewards(), &CollectionItemId::new("rw1"), toggle, on)
                .await
                .unwrap();

            assert_eq!(
                transport.request(0).body,
                Some(expected),
                "{toggle} -> {on}"
            );
        }
    }

    #[tokio::test]
    async fn a_write_answered_without_a_reward_row_reports_transport() {
        let (_, bundle) = bundle_answering(vec![Ok(json!({ "data": [] }))]);

        let outcome = bundle.create(&rewards(), &draft()).await;

        assert_eq!(outcome, Err(CollectionFailure::Transport));
    }

    #[tokio::test]
    async fn unknown_collection_and_unknown_toggle_are_refused_without_a_request() {
        let (transport, bundle) = bundle_answering(vec![]);
        let item = CollectionItemId::new("rw1");

        let unknown_collection = bundle.list(&CollectionId::new("scenes")).await;
        let unknown_toggle = bundle.set_toggle(&rewards(), &item, "in_stock", true).await;

        assert!(matches!(
            unknown_collection,
            Err(CollectionFailure::InvalidInput { field: None, .. })
        ));
        assert!(matches!(
            unknown_toggle,
            Err(CollectionFailure::InvalidInput { field: Some(ref key), .. }) if key == "in_stock"
        ));
        assert_eq!(transport.call_count(), 0);
    }

    #[tokio::test]
    async fn a_bundle_without_a_broadcaster_reports_not_connected_without_a_request() {
        let (transport, bundle) = bundle_with(vec![], BroadcasterTier::Affiliate, "");

        let outcome = bundle.list(&rewards()).await;

        assert_eq!(
            (outcome, transport.call_count()),
            (Err(CollectionFailure::NotConnected), 0)
        );
    }

    #[tokio::test]
    async fn invalid_drafts_are_rejected_on_the_offending_field_before_any_request() {
        let cases = [
            (with(TITLE_KEY, text("   ")), TITLE_KEY),
            (with(TITLE_KEY, text(&"a".repeat(46))), TITLE_KEY),
            (with(TITLE_KEY, text(&"ї".repeat(46))), TITLE_KEY),
            (with(COST_KEY, QuickActionFieldValue::Int(0)), COST_KEY),
            (with(COST_KEY, QuickActionFieldValue::Int(-1)), COST_KEY),
            (with(PROMPT_KEY, text(&"p".repeat(201))), PROMPT_KEY),
            (
                with(BACKGROUND_COLOR_KEY, text("9147FF")),
                BACKGROUND_COLOR_KEY,
            ),
            (
                with(BACKGROUND_COLOR_KEY, text("#9147F")),
                BACKGROUND_COLOR_KEY,
            ),
            (
                with(BACKGROUND_COLOR_KEY, text("#GG47FF")),
                BACKGROUND_COLOR_KEY,
            ),
            (
                with(MAX_PER_STREAM_KEY, QuickActionFieldValue::Int(-1)),
                MAX_PER_STREAM_KEY,
            ),
            (
                with(MAX_PER_USER_PER_STREAM_KEY, QuickActionFieldValue::Int(-1)),
                MAX_PER_USER_PER_STREAM_KEY,
            ),
            (
                with(GLOBAL_COOLDOWN_SECONDS_KEY, QuickActionFieldValue::Int(-1)),
                GLOBAL_COOLDOWN_SECONDS_KEY,
            ),
        ];
        for (values, expected_field) in cases {
            let (transport, bundle) = bundle_answering(vec![]);

            let outcome = bundle.create(&rewards(), &values).await;

            assert!(
                matches!(
                    &outcome,
                    Err(CollectionFailure::InvalidInput { field: Some(field), .. })
                        if field == expected_field
                ),
                "{expected_field}: {outcome:?}"
            );
            assert_eq!(transport.call_count(), 0, "{expected_field}");
        }
    }

    #[tokio::test]
    async fn drafts_on_the_limit_boundaries_are_sent() {
        let cases = [
            with(TITLE_KEY, text(&"a".repeat(45))),
            with(TITLE_KEY, text(&"ї".repeat(45))),
            with(COST_KEY, QuickActionFieldValue::Int(1)),
            with(PROMPT_KEY, text(&"p".repeat(200))),
            with(BACKGROUND_COLOR_KEY, text("")),
            with(MAX_PER_STREAM_KEY, QuickActionFieldValue::Int(0)),
            with(GLOBAL_COOLDOWN_SECONDS_KEY, QuickActionFieldValue::Int(0)),
        ];
        for values in cases {
            let (transport, bundle) = bundle_answering(vec![rows_of(vec![reward_row("a", "x")])]);

            let outcome = bundle.create(&rewards(), &values).await;

            assert!(outcome.is_ok(), "{values:?}: {outcome:?}");
            assert_eq!(transport.call_count(), 1);
        }
    }

    #[tokio::test]
    async fn a_missing_cost_falls_back_to_the_default_instead_of_failing() {
        let mut values = draft();
        values.remove(COST_KEY);
        let (transport, bundle) = bundle_answering(vec![rows_of(vec![reward_row("a", "x")])]);

        bundle.create(&rewards(), &values).await.unwrap();

        assert_eq!(
            transport.request(0).body.unwrap()[COST_KEY],
            json!(DEFAULT_REWARD_COST)
        );
    }

    #[tokio::test]
    async fn bad_request_bodies_map_to_field_level_invalid_input() {
        let cases: [(&str, CollectionFailure); 7] = [
            (
                "CREATE_CUSTOM_REWARD_DUPLICATE_REWARD",
                invalid(
                    TITLE_KEY,
                    "A reward with this title already exists".to_owned(),
                ),
            ),
            (
                "The title failed AutoMod checks",
                CollectionFailure::InvalidInput {
                    field: None,
                    message: "Twitch AutoMod rejected the reward text".to_owned(),
                },
            ),
            (
                "CREATE_CUSTOM_REWARD_TOO_MANY_REWARDS",
                CollectionFailure::CapacityReached,
            ),
            (
                "The parameter max_per_user_per_stream is invalid",
                invalid(
                    MAX_PER_USER_PER_STREAM_KEY,
                    "Twitch rejected the per-viewer limit".to_owned(),
                ),
            ),
            (
                "background_color is malformed",
                invalid(
                    BACKGROUND_COLOR_KEY,
                    "Twitch rejected the background color".to_owned(),
                ),
            ),
            (
                "Something unexpected",
                CollectionFailure::InvalidInput {
                    field: None,
                    message: "Twitch rejected the reward settings".to_owned(),
                },
            ),
            (
                "",
                CollectionFailure::InvalidInput {
                    field: None,
                    message: "Twitch rejected the reward settings".to_owned(),
                },
            ),
        ];
        for (message, expected) in cases {
            let (_, bundle) = bundle_answering(vec![http(StatusCode::BAD_REQUEST, message)]);

            let outcome = bundle.create(&rewards(), &draft()).await;

            assert_eq!(outcome, Err(expected), "{message:?}");
        }
    }

    #[tokio::test]
    async fn a_non_json_bad_request_body_still_maps_to_a_generic_invalid_input() {
        let (_, bundle) = bundle_answering(vec![Err(HelixError::Http {
            status: StatusCode::BAD_REQUEST.as_u16(),
            body: "<html>duplicate</html>".to_owned(),
        })]);

        let outcome = bundle.create(&rewards(), &draft()).await;

        assert!(matches!(
            outcome,
            Err(CollectionFailure::InvalidInput { field: None, .. })
        ));
    }

    #[tokio::test]
    async fn forbidden_maps_to_not_owned_or_not_eligible_by_call_and_tier() {
        let item = CollectionItemId::new("rw1");
        for (tier, message, expected_create, expected_modify) in [
            (
                BroadcasterTier::Affiliate,
                "",
                CollectionFailure::NotEligible,
                CollectionFailure::NotOwned,
            ),
            (
                BroadcasterTier::Partner,
                "",
                CollectionFailure::NotEligible,
                CollectionFailure::NotOwned,
            ),
            (
                BroadcasterTier::Standard,
                "",
                CollectionFailure::NotEligible,
                CollectionFailure::NotEligible,
            ),
            (
                BroadcasterTier::Affiliate,
                "maximum number of rewards reached",
                CollectionFailure::CapacityReached,
                CollectionFailure::NotOwned,
            ),
        ] {
            let (_, bundle) = bundle_with(
                vec![
                    http(StatusCode::FORBIDDEN, message),
                    http(StatusCode::FORBIDDEN, message),
                    http(StatusCode::FORBIDDEN, message),
                    http(StatusCode::FORBIDDEN, message),
                ],
                tier,
                SELF_USER_ID,
            );

            let create = bundle.create(&rewards(), &draft()).await;
            let update = bundle.update(&rewards(), &item, &draft()).await;
            let delete = bundle.delete(&rewards(), &item).await;
            let toggle = bundle
                .set_toggle(&rewards(), &item, ENABLED_TOGGLE, false)
                .await;

            assert_eq!(create, Err(expected_create), "{tier:?} create");
            assert_eq!(
                (update.map(|_| ()), delete, toggle.map(|_| ())),
                (
                    Err(expected_modify.clone()),
                    Err(expected_modify.clone()),
                    Err(expected_modify)
                ),
                "{tier:?} modify"
            );
        }
    }

    #[tokio::test]
    async fn forbidden_list_reports_the_channel_not_eligible() {
        let (_, bundle) = bundle_answering(vec![http(StatusCode::FORBIDDEN, "")]);

        let outcome = bundle.list(&rewards()).await;

        assert_eq!(outcome, Err(CollectionFailure::NotEligible));
    }

    #[tokio::test]
    async fn not_found_on_a_modify_reports_the_reward_gone_and_bumps_the_revision() {
        let (_, bundle) = bundle_answering(vec![http(StatusCode::NOT_FOUND, "")]);
        let mut revisions = bundle.revisions();

        let outcome = bundle
            .set_toggle(
                &rewards(),
                &CollectionItemId::new("rw1"),
                PAUSED_TOGGLE,
                true,
            )
            .await;

        assert_eq!(outcome, Err(reward_gone()));
        assert_eq!(
            tokio::time::timeout(std::time::Duration::ZERO, revisions.changed()).await,
            Ok(forge_platform_core::RevisionWait::Changed)
        );
    }

    #[tokio::test]
    async fn not_found_on_create_or_list_reports_transport() {
        let (_, bundle) = bundle_answering(vec![
            http(StatusCode::NOT_FOUND, ""),
            http(StatusCode::NOT_FOUND, ""),
        ]);

        let create = bundle.create(&rewards(), &draft()).await;
        let list = bundle.list(&rewards()).await;

        assert_eq!(
            (create, list),
            (
                Err(CollectionFailure::Transport),
                Err(CollectionFailure::Transport)
            )
        );
    }

    #[tokio::test]
    async fn transport_level_errors_map_to_their_coarse_failures() {
        for (error, expected) in [
            (HelixError::RateLimited, CollectionFailure::RateLimited),
            (HelixError::ReauthRequired, CollectionFailure::Unauthorized),
            (
                HelixError::Credentials("no token".to_owned()),
                CollectionFailure::NotConnected,
            ),
            (
                HelixError::Transport("timed out".to_owned()),
                CollectionFailure::Transport,
            ),
            (
                HelixError::Http {
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    body: String::new(),
                },
                CollectionFailure::Transport,
            ),
        ] {
            let (_, bundle) = bundle_answering(vec![Err(error)]);

            let outcome = bundle.list(&rewards()).await;

            assert_eq!(outcome, Err(expected));
        }
    }

    #[tokio::test]
    async fn no_failure_message_carries_the_token_the_url_or_the_raw_body() {
        let leaky = format!("{LEAKY_URL}?token={TOKEN_SENTINEL}");
        let errors = [
            HelixError::Transport(leaky.clone()),
            HelixError::Credentials(leaky.clone()),
            HelixError::Http {
                status: StatusCode::BAD_REQUEST.as_u16(),
                body: json!({ "message": format!("duplicate {leaky}") }).to_string(),
            },
            HelixError::Http {
                status: StatusCode::BAD_REQUEST.as_u16(),
                body: json!({ "message": format!("title {leaky}") }).to_string(),
            },
            HelixError::Http {
                status: StatusCode::BAD_REQUEST.as_u16(),
                body: json!({ "message": leaky.clone() }).to_string(),
            },
            HelixError::Http {
                status: StatusCode::FORBIDDEN.as_u16(),
                body: leaky.clone(),
            },
            HelixError::Http {
                status: StatusCode::BAD_GATEWAY.as_u16(),
                body: leaky.clone(),
            },
        ];
        for error in errors {
            let (_, bundle) = bundle_answering(vec![Err(error)]);

            let shown = bundle
                .create(&rewards(), &draft())
                .await
                .unwrap_err()
                .to_string();

            assert!(
                !shown.contains(TOKEN_SENTINEL) && !shown.contains("api.twitch.tv"),
                "leaked: {shown}"
            );
        }
    }

    #[tokio::test]
    async fn manager_writes_build_the_same_requests_as_the_reward_sub_actions() {
        let item = CollectionItemId::new("rw1");
        let config = BTreeMap::from([("reward_id".to_owned(), Variant::String("rw1".to_owned()))]);
        let stack = ArgStack::new();
        type RunnerFactory =
            fn(Arc<dyn HelixTransport>, Arc<SelfIdentity>) -> Box<dyn SubActionRunner>;
        type WriteCase = (&'static str, Option<(&'static str, bool)>, RunnerFactory);
        let cases: [WriteCase; 5] = [
            ("enable", Some((ENABLED_TOGGLE, true)), |t, i| {
                Box::new(EnableRewardRunner::new(t, i))
            }),
            ("disable", Some((ENABLED_TOGGLE, false)), |t, i| {
                Box::new(DisableRewardRunner::new(t, i))
            }),
            ("pause", Some((PAUSED_TOGGLE, true)), |t, i| {
                Box::new(PauseRewardRunner::new(t, i))
            }),
            ("resume", Some((PAUSED_TOGGLE, false)), |t, i| {
                Box::new(ResumeRewardRunner::new(t, i))
            }),
            ("delete", None, |t, i| {
                Box::new(DeleteRewardRunner::new(t, i))
            }),
        ];
        for (name, toggle, runner) in cases {
            let runner_transport = Arc::new(MockTransport::returning(Ok(Value::Null)));
            let runner = runner(
                Arc::clone(&runner_transport) as Arc<dyn HelixTransport>,
                Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
            );
            runner.execute(&config, &make_ctx(&stack)).await;
            let (manager_transport, bundle) =
                bundle_answering(vec![rows_of(vec![reward_row("rw1", "x")])]);
            match toggle {
                Some((key, on)) => {
                    bundle.set_toggle(&rewards(), &item, key, on).await.unwrap();
                }
                None => bundle.delete(&rewards(), &item).await.unwrap(),
            }

            let shape =
                |request: HelixRequest| (request.method, request.path, request.query, request.body);
            assert_eq!(
                shape(manager_transport.request(0)),
                shape(runner_transport.request(0)),
                "{name}"
            );
        }
    }
}
