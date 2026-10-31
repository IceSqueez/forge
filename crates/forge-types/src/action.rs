use serde::{Deserialize, Serialize};

use crate::ids::{ActionId, QueueId};
use crate::sub_action_step::SubActionStep;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    #[default]
    Sequential,
    RandomPick,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub id: ActionId,
    pub name: String,
    pub group: Option<String>,
    pub queue_id: QueueId,
    pub enabled: bool,
    pub concurrent: bool,
    pub bypass_pause: bool,
    #[serde(default)]
    pub execution_mode: ExecutionMode,
    pub description: Option<String>,
    pub sub_actions: Vec<SubActionStep>,
}
