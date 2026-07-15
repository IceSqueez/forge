use std::collections::HashSet;

use forge_events::Event;
use forge_types::QueueId;

pub struct QueueHealth {
    paused: HashSet<QueueId>,
}

impl QueueHealth {
    pub fn new() -> Self {
        Self {
            paused: HashSet::new(),
        }
    }

    /// `queue.cleared` deliberately advances nothing: the scheduler keeps pause state across a clear.
    pub fn apply_event(&mut self, event: &Event) -> bool {
        let Some(id) = queue_id_of(event) else {
            return false;
        };
        match event.kind.as_str() {
            "queue.paused" => self.paused.insert(id),
            "queue.resumed" => self.paused.remove(&id),
            _ => false,
        }
    }

    pub fn is_paused(&self, id: QueueId) -> bool {
        self.paused.contains(&id)
    }
}

fn queue_id_of(event: &Event) -> Option<QueueId> {
    event
        .payload
        .get("queue_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
}
