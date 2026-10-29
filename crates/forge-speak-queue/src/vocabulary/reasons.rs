use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    SkipRuleMatched,
    BlockedByWordFilter,
    EmptyAfterProcessing,
    BlockedByAlias,
    NoVoicesAvailable,
    VoiceOverrideNotFound,
    VoiceResolutionFailed,
    UserSkip,
    Promoted,
    Cancelled,
    Cleared,
}

impl SkipReason {
    pub const fn describe(self) -> &'static str {
        match self {
            SkipReason::SkipRuleMatched => "matched a skip rule",
            SkipReason::BlockedByWordFilter => "blocked by word filter",
            SkipReason::EmptyAfterProcessing => "empty after processing",
            SkipReason::BlockedByAlias => "blocked by alias",
            SkipReason::NoVoicesAvailable => "no voices available",
            SkipReason::VoiceOverrideNotFound => "voice override not found in catalog",
            SkipReason::VoiceResolutionFailed => "voice resolution failed",
            SkipReason::UserSkip => "skipped by user",
            SkipReason::Promoted => "promoted by user",
            SkipReason::Cancelled => "cancelled by its requester",
            SkipReason::Cleared => "stopped by clear",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureReason {
    EngineError,
    DeviceError,
    EngineTimeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    QueueFull,
    PerUserLimit,
}

impl RejectReason {
    pub fn describe(self, limit: usize) -> String {
        match self {
            RejectReason::QueueFull => format!("queue full (max {limit})"),
            RejectReason::PerUserLimit => format!("per-user limit (max {limit})"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HoldReason {
    User,
    VoiceGate,
}
