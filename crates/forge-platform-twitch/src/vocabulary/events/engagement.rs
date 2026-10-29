use forge_events::TypedEvent;
use serde::{Deserialize, Serialize};

use crate::vocabulary::event_kinds;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollChoice {
    pub id: String,
    pub title: String,
    pub votes: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollBegan {
    pub poll: PollOpening,
    pub choices: Vec<PollChoice>,
}

impl TypedEvent for PollBegan {
    const KIND: &'static str = event_kinds::CHANNEL_POLL_BEGIN;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollOpening {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub ends_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollProgressed {
    pub poll: PollHeading,
    pub choices: Vec<PollChoice>,
}

impl TypedEvent for PollProgressed {
    const KIND: &'static str = event_kinds::CHANNEL_POLL_PROGRESS;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollHeading {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollEnded {
    pub poll: PollClosing,
    pub choices: Vec<PollChoice>,
}

impl TypedEvent for PollEnded {
    const KIND: &'static str = event_kinds::CHANNEL_POLL_END;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollClosing {
    pub id: String,
    pub title: String,
    pub status: String,
    pub ended_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredictionOutcome {
    pub id: String,
    pub title: String,
    pub color: String,
    pub users: Option<i64>,
    pub channel_points: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredictionBegan {
    pub prediction: PredictionOpening,
    pub outcomes: Vec<PredictionOutcome>,
}

impl TypedEvent for PredictionBegan {
    const KIND: &'static str = event_kinds::CHANNEL_PREDICTION_BEGIN;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredictionOpening {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub locks_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredictionProgressed {
    pub prediction: PredictionHeading,
    pub outcomes: Vec<PredictionOutcome>,
}

impl TypedEvent for PredictionProgressed {
    const KIND: &'static str = event_kinds::CHANNEL_PREDICTION_PROGRESS;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredictionHeading {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredictionLocked {
    pub prediction: PredictionLock,
}

impl TypedEvent for PredictionLocked {
    const KIND: &'static str = event_kinds::CHANNEL_PREDICTION_LOCK;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredictionLock {
    pub id: String,
    pub title: String,
    pub locked_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredictionEnded {
    pub prediction: PredictionResolution,
}

impl TypedEvent for PredictionEnded {
    const KIND: &'static str = event_kinds::CHANNEL_PREDICTION_END;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredictionResolution {
    pub id: String,
    pub title: String,
    pub winning_outcome_id: String,
    pub status: String,
    pub ended_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalBegan {
    pub goal: GoalOpening,
}

impl TypedEvent for GoalBegan {
    const KIND: &'static str = event_kinds::CHANNEL_GOAL_BEGIN;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalOpening {
    pub id: String,
    #[serde(rename = "type")]
    pub goal_type: String,
    pub description: String,
    pub current_amount: i64,
    pub target_amount: i64,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalProgressed {
    pub goal: GoalProgress,
}

impl TypedEvent for GoalProgressed {
    const KIND: &'static str = event_kinds::CHANNEL_GOAL_PROGRESS;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalProgress {
    pub id: String,
    #[serde(rename = "type")]
    pub goal_type: String,
    pub current_amount: i64,
    pub target_amount: i64,
    pub is_synced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalEnded {
    pub goal: GoalClosing,
}

impl TypedEvent for GoalEnded {
    const KIND: &'static str = event_kinds::CHANNEL_GOAL_END;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalClosing {
    pub id: String,
    #[serde(rename = "type")]
    pub goal_type: String,
    pub current_amount: i64,
    pub target_amount: i64,
    pub is_achieved: bool,
    pub ended_at: String,
}
