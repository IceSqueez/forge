use crate::Event;

/// Synchronous, fire-and-forget event publication handle.
///
/// Implementors must not block the caller. `publish` queues or broadcasts the
/// event and returns immediately; slow consumers absorb their own lag.
pub trait EventPublisher: Send + Sync {
    fn publish(&self, event: Event);
}
