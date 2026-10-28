use std::sync::Arc;

use tokio::sync::broadcast;

use crate::{Event, EventPublisher, EventsError};

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
