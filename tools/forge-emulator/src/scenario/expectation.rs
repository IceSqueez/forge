use forge_events::EventSource;
use serde::{Deserialize, Serialize};

use super::matcher::{EventPattern, PayloadMatchers, UniqueMap};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Expectation {
    Event(ObservedEvent),
    EventAbsent(AbsentEvent),
    CausedBy(Causation),
    TwitchSubscription(TwitchSubscription),
    TwitchNoUnexpectedRequests {},
    TwitchRequestCount(RequestCount),
    OverlayContent(OverlayContent),
    LogLine(LogLine),
    DiscordPost(DiscordPost),
    ObsRequest(ObsRequestSeen),
    ObsAuth(ObsAuthOutcome),
    VtubeRequest(VTubeRequestSeen),
    VtubeAuth(VTubeAuthOutcome),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VTubeRequestSeen {
    pub message_type: String,
    #[serde(default)]
    pub data: PayloadMatchers,
    #[serde(default)]
    pub succeeded: Option<bool>,
    #[serde(default)]
    pub error_id: Option<i64>,
    pub within_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VTubeAuthOutcome {
    pub accepted: bool,
    pub within_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObsRequestSeen {
    pub request_type: String,
    #[serde(default)]
    pub request_data: PayloadMatchers,
    #[serde(default)]
    pub code: Option<u16>,
    pub within_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObsAuthOutcome {
    pub accepted: bool,
    pub within_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscordPost {
    pub webhook: String,
    #[serde(default)]
    pub content_contains: Option<String>,
    #[serde(default)]
    pub mention_parse: Option<Vec<String>>,
    pub within_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedEvent {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub source: Option<EventSource>,
    pub kind: String,
    #[serde(default)]
    pub payload: PayloadMatchers,
    #[serde(default)]
    pub count: ObservedCount,
    pub within_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ObservedCount {
    AtLeast(u32),
    Exactly(u32),
}

impl Default for ObservedCount {
    fn default() -> Self {
        Self::AtLeast(1)
    }
}

impl ObservedCount {
    pub fn minimum(self) -> u32 {
        match self {
            Self::AtLeast(count) | Self::Exactly(count) => count,
        }
    }

    pub fn is_single(self) -> bool {
        matches!(self, Self::AtLeast(1) | Self::Exactly(1))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbsentEvent {
    #[serde(default)]
    pub source: Option<EventSource>,
    pub kind: String,
    #[serde(default)]
    pub payload: PayloadMatchers,
    pub window_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Causation {
    pub effect: String,
    pub cause: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TwitchSubscription {
    #[serde(rename = "type")]
    pub subscription_type: String,
    #[serde(default)]
    pub version: Option<String>,
    pub within_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestCount {
    #[serde(default)]
    pub method: Option<String>,
    pub path: String,
    #[serde(default)]
    pub min: Option<u32>,
    #[serde(default)]
    pub max: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayContent {
    pub overlay: String,
    pub values: UniqueMap<String>,
    pub within_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogLine {
    pub target: String,
    pub fields: UniqueMap<String>,
    pub within_ms: u64,
}

impl ObservedEvent {
    pub fn pattern(&self) -> EventPattern<'_> {
        EventPattern {
            source: self.source,
            kind: &self.kind,
            payload: &self.payload,
        }
    }
}

impl AbsentEvent {
    pub fn pattern(&self) -> EventPattern<'_> {
        EventPattern {
            source: self.source,
            kind: &self.kind,
            payload: &self.payload,
        }
    }
}

impl Expectation {
    pub fn keyword(&self) -> &'static str {
        match self {
            Self::Event(_) => "event",
            Self::EventAbsent(_) => "event_absent",
            Self::CausedBy(_) => "caused_by",
            Self::TwitchSubscription(_) => "twitch_subscription",
            Self::TwitchNoUnexpectedRequests {} => "twitch_no_unexpected_requests",
            Self::TwitchRequestCount(_) => "twitch_request_count",
            Self::OverlayContent(_) => "overlay_content",
            Self::LogLine(_) => "log_line",
            Self::DiscordPost(_) => "discord_post",
            Self::ObsRequest(_) => "obs_request",
            Self::ObsAuth(_) => "obs_auth",
            Self::VtubeRequest(_) => "vtube_request",
            Self::VtubeAuth(_) => "vtube_auth",
        }
    }

    pub fn needs_fake_twitch(&self) -> bool {
        match self {
            Self::Event(event) => event.source == Some(EventSource::Twitch),
            Self::TwitchSubscription(_)
            | Self::TwitchNoUnexpectedRequests {}
            | Self::TwitchRequestCount(_) => true,
            Self::EventAbsent(_)
            | Self::CausedBy(_)
            | Self::OverlayContent(_)
            | Self::LogLine(_)
            | Self::DiscordPost(_)
            | Self::ObsRequest(_)
            | Self::ObsAuth(_)
            | Self::VtubeRequest(_)
            | Self::VtubeAuth(_) => false,
        }
    }

    pub fn needs_fake_discord(&self) -> bool {
        matches!(self, Self::DiscordPost(_))
    }

    pub fn needs_fake_obs(&self) -> bool {
        matches!(self, Self::ObsRequest(_) | Self::ObsAuth(_))
    }

    pub fn needs_fake_vtube(&self) -> bool {
        matches!(self, Self::VtubeRequest(_) | Self::VtubeAuth(_))
    }
}
