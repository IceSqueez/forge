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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedEvent {
    /// Lets a `caused_by` expectation refer to the event this one matched.
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

/// Holds when the event named `effect` carries the id of the event named `cause` as its direct cause.
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

/// Counts fake Twitch REST requests by exact path and, when given, method.
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

/// Holds when the page open for `overlay` receives a content frame carrying every named key as
/// exactly that plain JSON string. A frame whose content holds a tagged `{"type","value"}` object
/// anywhere fails this expectation even when the named keys read correctly: a page renders what
/// the wire gives it, and a tagged object renders as `[object Object]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayContent {
    pub overlay: String,
    pub values: UniqueMap<String>,
    pub within_ms: u64,
}

/// Matches structured field values as rendered in the log; never the message prose.
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
            | Self::LogLine(_) => false,
        }
    }
}
