#[derive(Debug, thiserror::Error)]
pub enum EventsError {
    #[error("event bus is closed")]
    BusClosed,

    #[error("subscriber is lagging; events were dropped")]
    LaggingReceiver,
}
