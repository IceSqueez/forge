use forge_types::TriggerInstanceId;
use serde::{Deserialize, Serialize};

use crate::TypedEvent;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerTick {
    pub instance_id: TriggerInstanceId,
    pub interval_minutes: u32,
    pub message_count: u32,
}

impl TypedEvent for TimerTick {
    const KIND: &'static str = "timer.tick";
}
