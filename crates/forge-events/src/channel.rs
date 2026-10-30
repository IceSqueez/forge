use std::sync::Arc;

use tokio::sync::broadcast;

use crate::{Event, EventPublisher, EventsError};

pub const PLATFORM_EVENT_CHANNEL_CAPACITY: usize = 4096;

pub enum Delivery {
    Event(Arc<Event>),
    Skipped(u64),
    Closed,
}

pub trait LagRecorder: Send + Sync {
    fn lagged(&self, skipped: u64);
}

pub trait EventObserver: Send + Sync {
    fn observe(&self, consumer: &'static str) -> EventSubscription;
}

pub struct EventChannel {
    sender: broadcast::Sender<Arc<Event>>,
}

impl EventChannel {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self { sender }
    }

    pub fn subscribe(&self) -> EventSubscription {
        EventSubscription {
            receiver: self.sender.subscribe(),
            lag: None,
        }
    }

    pub fn subscribe_recording(&self, lag: Arc<dyn LagRecorder>) -> EventSubscription {
        EventSubscription {
            receiver: self.sender.subscribe(),
            lag: Some(lag),
        }
    }

    pub fn send(&self, event: Arc<Event>) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "having no subscribers is a normal state"
        )]
        let _ = self.sender.send(event);
    }
}

impl EventPublisher for EventChannel {
    fn publish(&self, event: Event) {
        self.send(Arc::new(event));
    }
}

pub struct EventSubscription {
    receiver: broadcast::Receiver<Arc<Event>>,
    lag: Option<Arc<dyn LagRecorder>>,
}

impl EventSubscription {
    pub fn recording_lag(mut self, lag: Arc<dyn LagRecorder>) -> Self {
        self.lag = Some(lag);
        self
    }

    pub async fn next(&mut self) -> Delivery {
        match self.receiver.recv().await {
            Ok(event) => Delivery::Event(event),
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                self.record_lag(skipped);
                Delivery::Skipped(skipped)
            }
            Err(broadcast::error::RecvError::Closed) => Delivery::Closed,
        }
    }

    pub async fn recv(&mut self) -> Result<Event, EventsError> {
        match self.next().await {
            Delivery::Event(event) => Ok(Arc::unwrap_or_clone(event)),
            Delivery::Skipped(_) => Err(EventsError::LaggingReceiver),
            Delivery::Closed => Err(EventsError::BusClosed),
        }
    }

    pub fn try_recv(&mut self) -> Result<Option<Event>, EventsError> {
        match self.receiver.try_recv() {
            Ok(event) => Ok(Some(Arc::unwrap_or_clone(event))),
            Err(broadcast::error::TryRecvError::Empty) => Ok(None),
            Err(broadcast::error::TryRecvError::Closed) => Err(EventsError::BusClosed),
            Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                self.record_lag(skipped);
                Err(EventsError::LaggingReceiver)
            }
        }
    }

    fn record_lag(&self, skipped: u64) {
        if let Some(lag) = &self.lag {
            lag.lagged(skipped);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::EventSource;

    const SMALL_CAPACITY: usize = 4;
    const OVERFLOWING_SENDS: usize = 10;
    const EXPECTED_SKIPPED: u64 = (OVERFLOWING_SENDS - SMALL_CAPACITY) as u64;

    #[derive(Default)]
    struct RecordedLags(Mutex<Vec<u64>>);

    impl RecordedLags {
        fn snapshot(&self) -> Vec<u64> {
            self.0.lock().map(|lags| lags.clone()).unwrap_or_default()
        }
    }

    impl LagRecorder for RecordedLags {
        fn lagged(&self, skipped: u64) {
            if let Ok(mut lags) = self.0.lock() {
                lags.push(skipped);
            }
        }
    }

    fn numbered(index: usize) -> Event {
        Event::new(
            EventSource::Core,
            "flood",
            serde_json::json!({ "i": index }),
        )
    }

    fn overflow(channel: &EventChannel) {
        for index in 0..OVERFLOWING_SENDS {
            channel.publish(numbered(index));
        }
    }

    #[tokio::test]
    async fn published_event_reaches_every_subscriber_as_one_shared_allocation() {
        let channel = EventChannel::new(SMALL_CAPACITY);
        let mut first = channel.subscribe();
        let mut second = channel.subscribe();
        channel.publish(numbered(0));
        let (Delivery::Event(a), Delivery::Event(b)) = (first.next().await, second.next().await)
        else {
            panic!("both subscribers must receive the event");
        };
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[tokio::test]
    async fn overflow_reports_exact_skipped_count() {
        let channel = EventChannel::new(SMALL_CAPACITY);
        let mut subscription = channel.subscribe();
        overflow(&channel);
        assert!(matches!(
            subscription.next().await,
            Delivery::Skipped(EXPECTED_SKIPPED)
        ));
    }

    #[tokio::test]
    async fn delivery_resumes_at_oldest_retained_event_after_skip() {
        let channel = EventChannel::new(SMALL_CAPACITY);
        let mut subscription = channel.subscribe();
        overflow(&channel);
        let _skipped = subscription.next().await;
        let Delivery::Event(event) = subscription.next().await else {
            panic!("an event must follow the skip notice");
        };
        assert_eq!(event.payload["i"], serde_json::json!(EXPECTED_SKIPPED));
    }

    #[tokio::test]
    async fn recv_after_overflow_returns_lagging_receiver() {
        let channel = EventChannel::new(SMALL_CAPACITY);
        let mut subscription = channel.subscribe();
        overflow(&channel);
        assert!(matches!(
            subscription.recv().await,
            Err(EventsError::LaggingReceiver)
        ));
    }

    #[tokio::test]
    async fn recording_subscription_reports_skipped_count_on_next() {
        let channel = EventChannel::new(SMALL_CAPACITY);
        let lags = Arc::new(RecordedLags::default());
        let mut subscription = channel.subscribe_recording(lags.clone());
        overflow(&channel);
        let _skipped = subscription.next().await;
        assert_eq!(lags.snapshot(), vec![EXPECTED_SKIPPED]);
    }

    #[test]
    fn recording_subscription_reports_skipped_count_on_try_recv() {
        let channel = EventChannel::new(SMALL_CAPACITY);
        let lags = Arc::new(RecordedLags::default());
        let mut subscription = channel.subscribe_recording(lags.clone());
        overflow(&channel);
        assert!(matches!(
            subscription.try_recv(),
            Err(EventsError::LaggingReceiver)
        ));
        assert_eq!(lags.snapshot(), vec![EXPECTED_SKIPPED]);
    }

    #[tokio::test]
    async fn lag_on_plain_subscription_is_not_recorded_by_sibling_recorder() {
        let channel = EventChannel::new(SMALL_CAPACITY);
        let lags = Arc::new(RecordedLags::default());
        let _recording = channel.subscribe_recording(lags.clone());
        let mut plain = channel.subscribe();
        overflow(&channel);
        let _skipped = plain.next().await;
        assert!(lags.snapshot().is_empty());
    }

    #[tokio::test]
    async fn dropped_channel_delivers_closed() {
        let channel = EventChannel::new(SMALL_CAPACITY);
        let mut subscription = channel.subscribe();
        drop(channel);
        assert!(matches!(subscription.next().await, Delivery::Closed));
    }

    #[tokio::test]
    async fn recv_on_dropped_channel_returns_bus_closed() {
        let channel = EventChannel::new(SMALL_CAPACITY);
        let mut subscription = channel.subscribe();
        drop(channel);
        assert!(matches!(
            subscription.recv().await,
            Err(EventsError::BusClosed)
        ));
    }

    #[test]
    fn try_recv_on_dropped_channel_returns_bus_closed() {
        let channel = EventChannel::new(SMALL_CAPACITY);
        let mut subscription = channel.subscribe();
        drop(channel);
        assert!(matches!(
            subscription.try_recv(),
            Err(EventsError::BusClosed)
        ));
    }

    #[test]
    fn try_recv_on_empty_open_channel_returns_none() {
        let channel = EventChannel::new(SMALL_CAPACITY);
        let mut subscription = channel.subscribe();
        assert!(matches!(subscription.try_recv(), Ok(None)));
    }

    #[test]
    fn zero_capacity_channel_still_holds_one_event() {
        let channel = EventChannel::new(0);
        let mut subscription = channel.subscribe();
        channel.publish(numbered(0));
        assert!(matches!(subscription.try_recv(), Ok(Some(_))));
    }
}
