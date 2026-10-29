use forge_events::TypedEvent;
use serde::{Deserialize, Serialize};

use super::parties::{TwitchBroadcaster, TwitchUser};
use crate::vocabulary::event_kinds;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamWentOnline {
    pub stream: LiveStream,
    pub broadcaster: TwitchBroadcaster,
}

impl TypedEvent for StreamWentOnline {
    const KIND: &'static str = event_kinds::STREAM_ONLINE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveStream {
    pub id: String,
    #[serde(rename = "type")]
    pub stream_type: String,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamWentOffline {
    pub broadcaster: TwitchBroadcaster,
}

impl TypedEvent for StreamWentOffline {
    const KIND: &'static str = event_kinds::STREAM_OFFLINE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelUpdated {
    pub channel: ChannelInfo,
}

impl TypedEvent for ChannelUpdated {
    const KIND: &'static str = event_kinds::CHANNEL_UPDATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub title: String,
    pub language: String,
    pub category_id: String,
    pub category_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdBreakBegan {
    pub ad_break: AdBreak,
    pub requester: TwitchUser,
}

impl TypedEvent for AdBreakBegan {
    const KIND: &'static str = event_kinds::CHANNEL_AD_BREAK_BEGIN;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdBreak {
    pub duration_seconds: i64,
    pub is_automatic: bool,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShoutoutSent {
    pub to_broadcaster: TwitchUser,
    pub viewer_count: i64,
    pub started_at: String,
}

impl TypedEvent for ShoutoutSent {
    const KIND: &'static str = event_kinds::CHANNEL_SHOUTOUT_CREATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShoutoutReceived {
    pub from_broadcaster: TwitchUser,
    pub viewer_count: i64,
    pub started_at: String,
}

impl TypedEvent for ShoutoutReceived {
    const KIND: &'static str = event_kinds::CHANNEL_SHOUTOUT_RECEIVE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestStarSessionBegan {
    pub session: GuestStarSessionOpening,
}

impl TypedEvent for GuestStarSessionBegan {
    const KIND: &'static str = event_kinds::CHANNEL_GUEST_STAR_SESSION_BEGIN;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestStarSessionOpening {
    pub id: String,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestStarSessionEnded {
    pub session: GuestStarSessionSpan,
}

impl TypedEvent for GuestStarSessionEnded {
    const KIND: &'static str = event_kinds::CHANNEL_GUEST_STAR_SESSION_END;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestStarSessionSpan {
    pub id: String,
    pub started_at: String,
    pub ended_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestStarSettingsUpdated {
    pub settings: GuestStarSettings,
}

impl TypedEvent for GuestStarSettingsUpdated {
    const KIND: &'static str = event_kinds::CHANNEL_GUEST_STAR_SETTINGS_UPDATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestStarSettings {
    pub slot_count: i64,
    pub group_layout: String,
    pub is_moderator_send_live_enabled: bool,
    pub is_browser_source_audio_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestStarGuestUpdated {
    pub session_id: String,
    pub slot_id: String,
    pub state: String,
    pub guest: GuestStarGuest,
    pub host: GuestStarHost,
}

impl TypedEvent for GuestStarGuestUpdated {
    const KIND: &'static str = event_kinds::CHANNEL_GUEST_STAR_GUEST_UPDATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestStarGuest {
    pub id: Option<String>,
    pub login: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestStarHost {
    pub video_enabled: bool,
    pub audio_enabled: bool,
    pub volume: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserUpdated {
    pub user: UpdatedUser,
}

impl TypedEvent for UserUpdated {
    const KIND: &'static str = event_kinds::USER_UPDATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatedUser {
    pub id: String,
    pub login: String,
    pub display_name: String,
    pub description: String,
}
