use forge_events::TypedEvent;
use serde::{Deserialize, Serialize};

use super::parties::TwitchUser;
use crate::vocabulary::event_kinds;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardRedeemed {
    pub redemption: Redemption,
    pub user: TwitchUser,
    pub reward: RedeemedReward,
}

impl TypedEvent for RewardRedeemed {
    const KIND: &'static str = event_kinds::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REDEMPTION_ADD;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redemption {
    pub id: String,
    pub status: String,
    pub user_input: String,
    pub redeemed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedeemedReward {
    pub id: String,
    pub title: String,
    pub cost: i64,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedemptionUpdated {
    pub redemption: Redemption,
    pub user: TwitchUser,
    pub reward: UpdatedRedemptionReward,
}

impl TypedEvent for RedemptionUpdated {
    const KIND: &'static str = event_kinds::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REDEMPTION_UPDATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatedRedemptionReward {
    pub id: String,
    pub title: String,
    pub cost: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomRewardState {
    pub reward: CustomReward,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomReward {
    pub id: String,
    pub title: String,
    pub cost: i64,
    pub prompt: String,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CustomRewardAdded(pub CustomRewardState);

impl TypedEvent for CustomRewardAdded {
    const KIND: &'static str = event_kinds::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_ADD;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CustomRewardUpdated(pub CustomRewardState);

impl TypedEvent for CustomRewardUpdated {
    const KIND: &'static str = event_kinds::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_UPDATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CustomRewardRemoved(pub CustomRewardState);

impl TypedEvent for CustomRewardRemoved {
    const KIND: &'static str = event_kinds::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REMOVE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomaticRewardRedeemed {
    pub redemption: AutomaticRedemption,
    pub user: TwitchUser,
    pub reward: AutomaticReward,
}

impl TypedEvent for AutomaticRewardRedeemed {
    const KIND: &'static str = event_kinds::CHANNEL_CHANNEL_POINTS_AUTOMATIC_REWARD_REDEMPTION_ADD;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomaticRedemption {
    pub id: String,
    pub redeemed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomaticReward {
    #[serde(rename = "type")]
    pub reward_type: String,
    pub cost: i64,
}
