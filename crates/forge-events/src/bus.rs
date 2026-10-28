use forge_types::EventId;

#[derive(Debug, thiserror::Error)]
pub enum EventsError {
    #[error("event bus is closed")]
    BusClosed,

    #[error("event {0} not found in replay buffer")]
    ReplayMiss(EventId),

    #[error("subscriber is lagging; events were dropped")]
    LaggingReceiver,
}
