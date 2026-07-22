use forge_events::{Event, EventPublisher, EventStream};
use tokio::sync::broadcast;

/// Buffered slots before a lagging bridge starts dropping the oldest events.
const CHANNEL_CAPACITY: usize = 256;

pub(crate) struct PlatformEventChannel {
    sender: broadcast::Sender<Event>,
}

impl PlatformEventChannel {
    pub(crate) fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { sender }
    }

    pub(crate) fn subscribe(&self) -> EventStream {
        EventStream::new(self.sender.subscribe())
    }
}

impl EventPublisher for PlatformEventChannel {
    fn publish(&self, event: Event) {
        let _ = self.sender.send(event);
    }
}
