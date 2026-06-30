use forge_events::{Event, EventPublisher, EventStream};
use tokio::sync::broadcast;

/// Buffered slots before a lagging bridge starts dropping the oldest events.
const CHANNEL_CAPACITY: usize = 256;

/// The platform's own outbound event origin.
///
/// Twitch chat sessions and the Helix transport publish their events here;
/// [`TwitchPlatform::events`](crate::TwitchPlatform::events) hands the receiving
/// half to the runtime, which bridges it onto the global bus. The platform never
/// reaches into the runtime — it owns the channel and exposes only the stream.
pub(crate) struct PlatformEventChannel {
    sender: broadcast::Sender<Event>,
}

impl PlatformEventChannel {
    pub(crate) fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { sender }
    }

    /// A fresh subscriber to this platform's event stream.
    pub(crate) fn subscribe(&self) -> EventStream {
        EventStream::new(self.sender.subscribe())
    }
}

impl EventPublisher for PlatformEventChannel {
    fn publish(&self, event: Event) {
        // A send error means no bridge is currently subscribed; the event is
        // dropped rather than blocking the producing task.
        let _ = self.sender.send(event);
    }
}
