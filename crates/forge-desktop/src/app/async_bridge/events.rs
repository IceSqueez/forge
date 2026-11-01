use forge_events::{Event, EventSubscription, EventsError};
use forge_runtime::EventBus;
use gpui::AsyncApp;

const BRIDGE_DRAIN_CAP: usize = 128;

pub(crate) enum EventBatch {
    Ready(Vec<Event>),
    Closed,
}

pub(crate) async fn recv_event_batch(sub: &mut EventSubscription) -> EventBatch {
    let first = loop {
        match sub.recv().await {
            Ok(event) => break event,
            Err(EventsError::LaggingReceiver) => {
                tracing::warn!("event bridge lagged; dropped events");
                continue;
            }
            Err(_) => return EventBatch::Closed,
        }
    };
    let mut batch = Vec::with_capacity(1);
    batch.push(first);
    while batch.len() < BRIDGE_DRAIN_CAP {
        match sub.try_recv() {
            Ok(Some(event)) => batch.push(event),
            _ => break,
        }
    }
    EventBatch::Ready(batch)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BridgeFlow {
    Continue,
    Stop,
}

pub(crate) async fn drain_events<F>(bus: &EventBus, cx: &mut AsyncApp, apply: F)
where
    F: FnMut(&[Event], &mut AsyncApp) -> BridgeFlow,
{
    drain_subscription(bus.subscribe(), cx, apply).await;
}

pub(crate) async fn drain_subscription<F>(
    mut sub: EventSubscription,
    cx: &mut AsyncApp,
    mut apply: F,
) where
    F: FnMut(&[Event], &mut AsyncApp) -> BridgeFlow,
{
    while let EventBatch::Ready(batch) = recv_event_batch(&mut sub).await {
        if apply(&batch, cx) == BridgeFlow::Stop {
            break;
        }
    }
}
