use std::sync::{Mutex, PoisonError};

use crate::{Event, EventPublisher};

pub struct NullPublisher;

impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

#[derive(Default)]
pub struct CapturingPublisher {
    events: Mutex<Vec<Event>>,
}

impl CapturingPublisher {
    pub fn events(&self) -> Vec<Event> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl EventPublisher for CapturingPublisher {
    fn publish(&self, event: Event) {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event);
    }
}
