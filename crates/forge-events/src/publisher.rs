use crate::Event;

pub trait EventPublisher: Send + Sync {
    fn publish(&self, event: Event);
}
