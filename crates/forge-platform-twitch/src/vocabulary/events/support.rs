use forge_events::TypedEvent;
use serde::{Deserialize, Serialize};

use super::parties::TwitchUser;
use crate::vocabulary::RaidDirection;
use crate::vocabulary::event_kinds;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelSubscribed {
    pub user: TwitchUser,
    pub tier: String,
    pub is_gift: bool,
}

impl TypedEvent for ChannelSubscribed {
    const KIND: &'static str = event_kinds::CHANNEL_SUBSCRIBE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelResubscribed {
    pub user: TwitchUser,
    pub tier: String,
    pub cumulative_months: i64,
    pub streak_months: i64,
    pub message: String,
    pub share_streak: bool,
}

impl TypedEvent for ChannelResubscribed {
    const KIND: &'static str = event_kinds::CHANNEL_SUBSCRIPTION_MESSAGE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionsGifted {
    pub tier: String,
    pub is_anonymous: bool,
    pub gift_total: i64,
    pub gifter: TwitchUser,
}

impl TypedEvent for SubscriptionsGifted {
    const KIND: &'static str = event_kinds::CHANNEL_SUBSCRIPTION_GIFT;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BitsCheered {
    pub bits: i64,
    pub message: String,
    pub is_anonymous: bool,
    pub user: TwitchUser,
}

impl TypedEvent for BitsCheered {
    const KIND: &'static str = event_kinds::CHANNEL_CHEER;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelRaided {
    pub direction: RaidDirection,
    pub viewer_count: i64,
    pub from_broadcaster: TwitchUser,
    pub to_broadcaster: TwitchUser,
}

impl TypedEvent for ChannelRaided {
    const KIND: &'static str = event_kinds::CHANNEL_RAID;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelFollowed {
    pub followed_at: String,
    pub user: TwitchUser,
}

impl TypedEvent for ChannelFollowed {
    const KIND: &'static str = event_kinds::CHANNEL_FOLLOW;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharityDonated {
    pub charity: CharityDonation,
    pub user: TwitchUser,
}

impl TypedEvent for CharityDonated {
    const KIND: &'static str = event_kinds::CHANNEL_CHARITY_CAMPAIGN_DONATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharityDonation {
    pub id: String,
    pub name: String,
    pub description: String,
    pub website: String,
    pub amount_cents: i64,
    pub currency_code: String,
    #[serde(default)]
    pub decimal_places: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharityCampaignState {
    pub charity: CharityCampaignTotals,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharityCampaignTotals {
    pub id: String,
    pub name: String,
    pub current_amount_cents: i64,
    pub target_amount_cents: i64,
    pub currency_code: String,
    #[serde(default)]
    pub decimal_places: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CharityCampaignStarted(pub CharityCampaignState);

impl TypedEvent for CharityCampaignStarted {
    const KIND: &'static str = event_kinds::CHANNEL_CHARITY_CAMPAIGN_START;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CharityCampaignProgressed(pub CharityCampaignState);

impl TypedEvent for CharityCampaignProgressed {
    const KIND: &'static str = event_kinds::CHANNEL_CHARITY_CAMPAIGN_PROGRESS;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CharityCampaignStopped(pub CharityCampaignState);

impl TypedEvent for CharityCampaignStopped {
    const KIND: &'static str = event_kinds::CHANNEL_CHARITY_CAMPAIGN_STOP;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypeTrainBegan {
    pub hype: HypeTrainOpening,
}

impl TypedEvent for HypeTrainBegan {
    const KIND: &'static str = event_kinds::CHANNEL_HYPE_TRAIN_BEGIN;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypeTrainOpening {
    pub id: String,
    pub level: i64,
    pub total: i64,
    pub goal: i64,
    pub progress: i64,
    pub started_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypeTrainProgressed {
    pub hype: HypeTrainProgress,
}

impl TypedEvent for HypeTrainProgressed {
    const KIND: &'static str = event_kinds::CHANNEL_HYPE_TRAIN_PROGRESS;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypeTrainProgress {
    pub id: String,
    pub level: i64,
    pub total: i64,
    pub goal: i64,
    pub progress: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypeTrainEnded {
    pub hype: HypeTrainClosing,
}

impl TypedEvent for HypeTrainEnded {
    const KIND: &'static str = event_kinds::CHANNEL_HYPE_TRAIN_END;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypeTrainClosing {
    pub id: String,
    pub level: i64,
    pub total: i64,
    pub ended_at: String,
    pub cooldown_ends_at: String,
}
