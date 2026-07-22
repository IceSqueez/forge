use crate::Event;

/// Implementors must not block the caller; slow consumers absorb their own lag.
pub trait EventPublisher: Send + Sync {
    fn publish(&self, event: Event);
}
