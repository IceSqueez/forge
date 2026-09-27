use forge_events::{Event, EventPublisher, EventStream};
use tokio::sync::broadcast;

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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_events::EventSource;

    use super::*;

    #[tokio::test]
    async fn published_event_reaches_a_prior_subscriber() {
        let channel = PlatformEventChannel::new();
        let mut stream = channel.subscribe();
        let event = Event::new(
            EventSource::Kick,
            "kick.chat.message.sent",
            serde_json::Value::Null,
        );
        let sent_id = event.id;
        channel.publish(event);
        let received = stream.recv().await.unwrap();
        assert_eq!(received.id, sent_id);
    }
}
