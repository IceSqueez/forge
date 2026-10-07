use forge_events::{Event, EventPublisher, EventStream};
use tokio::sync::broadcast;

const CHANNEL_CAPACITY: usize = 4096;

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
    use super::*;
    use forge_events::EventSource;

    const RAID_BURST: usize = 1000;

    #[tokio::test]
    async fn a_burst_published_before_the_bridge_reads_reaches_it_whole_and_in_order() {
        let channel = PlatformEventChannel::new();
        let mut stream = channel.subscribe();

        for index in 0..RAID_BURST {
            channel.publish(Event::new(
                EventSource::Twitch,
                "twitch.channel.chat.message",
                serde_json::json!({ "index": index }),
            ));
        }

        for index in 0..RAID_BURST {
            let event = stream.recv().await.unwrap();
            assert_eq!(event.payload["index"], index);
        }
    }
}
