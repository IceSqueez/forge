use forge_events::EventPublisher;
use forge_types::{ArgStack, EventId};

pub struct RunContext<'a> {
    pub arg_stack: &'a ArgStack,
    pub index: usize,
    pub parent_event_id: EventId,
    pub publisher: &'a dyn EventPublisher,
}
