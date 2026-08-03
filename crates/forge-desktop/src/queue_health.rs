use std::collections::HashMap;

use forge_events::Event;
use forge_runtime::{QueueIntake, QueueMode, QueueProcessing};
use forge_types::QueueId;

pub struct QueueHealth {
    modes: HashMap<QueueId, QueueMode>,
}

impl QueueHealth {
    pub fn new() -> Self {
        Self {
            modes: HashMap::new(),
        }
    }

    pub fn apply_event(&mut self, event: &Event) -> bool {
        let Some(id) = queue_id_of(event) else {
            return false;
        };
        let Some(mode) = mode_of(event) else {
            return false;
        };
        self.modes.insert(id, mode) != Some(mode)
    }

    pub fn mode(&self, id: QueueId) -> Option<QueueMode> {
        self.modes.get(&id).copied()
    }
}

fn queue_id_of(event: &Event) -> Option<QueueId> {
    event
        .payload
        .get("queue_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
}

fn mode_of(event: &Event) -> Option<QueueMode> {
    let processing = match event.payload.get("processing")?.as_str()? {
        "running" => QueueProcessing::Running,
        "frozen" => QueueProcessing::Frozen,
        _ => return None,
    };
    let intake = match event.payload.get("intake")?.as_str()? {
        "accept" => QueueIntake::Accept,
        "skip" => QueueIntake::Skip,
        _ => return None,
    };
    Some(QueueMode { processing, intake })
}
