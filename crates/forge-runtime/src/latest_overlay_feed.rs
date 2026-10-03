use std::sync::Arc;

use forge_events::LatestChanged;

use crate::bus::{Delivery, EventBus};
use crate::delivery::LATEST_OVERLAYS;
use crate::overlay_service::OverlayServiceHandle;

pub fn spawn_latest_overlay_feed(bus: &Arc<EventBus>, overlays: OverlayServiceHandle) {
    let mut subscription = bus.subscribe_observer(LATEST_OVERLAYS);
    tokio::spawn(async move {
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
