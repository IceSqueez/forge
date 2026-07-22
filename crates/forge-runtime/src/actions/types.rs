use forge_types::{Action, ActionId, SubActionOutcome, TriggerInstance};
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct ActionSummary {
    pub id: ActionId,
    pub name: String,
    pub enabled: bool,
    pub sub_action_count: u16,
    /// When set, buckets the action under a custom sidebar group rather than a trigger-derived category.
    pub group: Option<String>,
    pub first_trigger_kind_id: Option<String>,
    pub queue_name: String,
    pub last_ran: Option<OffsetDateTime>,
    pub runs_24h: u32,
}

#[derive(Debug, Clone)]
pub struct ActionDetail {
    pub action: Action,
    pub trigger_instances: Vec<TriggerInstance>,
    pub sub_action_avg_ms: Vec<Option<u64>>,
    pub last_step_outcomes: Vec<Option<SubActionOutcome>>,
}
