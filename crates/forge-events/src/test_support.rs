#![allow(clippy::expect_used)]

use std::sync::Arc;

use tokio::sync::{mpsc, watch};

use crate::{Event, EventPublisher};

pub struct NullPublisher;

impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

pub struct ForwardingPublisher(pub mpsc::UnboundedSender<Event>);

impl EventPublisher for ForwardingPublisher {
    fn publish(&self, event: Event) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the receiver may already be gone"
        )]
        let _ = self.0.send(event);
    }
}

#[derive(Default)]
pub struct CapturingPublisher {
    events: watch::Sender<Vec<Event>>,
}

impl CapturingPublisher {
    pub fn shared() -> Arc<Self> {
        Arc::default()
    }

    pub fn events(&self) -> Vec<Event> {
        self.events.borrow().clone()
    }

    pub fn kinds(&self) -> Vec<String> {
        self.events
            .borrow()
            .iter()
            .map(|event| event.kind.clone())
            .collect()
    }

    pub fn of_kind(&self, kind: &str) -> Vec<Event> {
        self.events
            .borrow()
            .iter()
            .filter(|event| event.kind == kind)
            .cloned()
            .collect()
    }

    pub fn find_kind(&self, kind: &str) -> Option<Event> {
        self.events
            .borrow()
            .iter()
            .find(|event| event.kind == kind)
            .cloned()
    }

    pub fn has_kind(&self, kind: &str) -> bool {
        self.events.borrow().iter().any(|event| event.kind == kind)
    }

    pub fn count_kind(&self, kind: &str) -> usize {
        self.of_kind(kind).len()
    }

    pub fn take(&self) -> Vec<Event> {
        self.events.send_replace(Vec::new())
    }

    pub async fn wait_for(&self, mut matches: impl FnMut(&Event) -> bool) -> Event {
        let mut found = None;
        self.events
            .subscribe()
            .wait_for(|events| {
                found = events.iter().find(|event| matches(event)).cloned();
                found.is_some()
            })
            .await
            .expect("the publisher owns the sender for as long as this borrow lives");
        found.expect("wait_for only returns once a match was stored")
    }

    pub async fn wait_for_kind(&self, kind: &str) -> Event {
        self.wait_for(|event| event.kind == kind).await
    }
}

impl EventPublisher for CapturingPublisher {
    fn publish(&self, event: Event) {
        self.events.send_modify(|events| events.push(event));
    }
}
