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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HoldReason {
    User,
    VoiceGate,
}
