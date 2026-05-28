use forge_types::{Action, ActionId, TriggerInstance};
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct ActionSummary {
    pub id: ActionId,
    pub name: String,
    pub enabled: bool,
    pub sub_action_count: u16,
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
}
