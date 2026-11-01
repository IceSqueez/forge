use std::sync::Arc;

use forge_events::{LatestChanged, TypedEvent};
use forge_net::TaskOwner;

use crate::delivery::LATEST_OVERLAYS;
use crate::event_flow::bus::EventBus;
use crate::overlays::overlay_service::OverlayServiceHandle;
use forge_events::Delivery;

pub fn spawn_latest_overlay_feed(
    bus: &Arc<EventBus>,
    overlays: OverlayServiceHandle,
    tasks: &TaskOwner,
) {
    let mut subscription = bus.subscribe_observer(LATEST_OVERLAYS);
    tasks.spawn_until_stopped(async move {
        loop {
            match subscription.next().await {
                Delivery::Event(event) => {
                    if let Some(changed) = LatestChanged::from_event(&event) {
                        overlays.refresh_latest(Some(&changed.slot)).await;
                    }
                }
                Delivery::Skipped(_) => overlays.refresh_latest(None).await,
                Delivery::Closed => break,
            }
        }
    });
}
