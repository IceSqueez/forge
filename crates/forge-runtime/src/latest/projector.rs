use std::sync::Arc;

use crate::bus::EventBus;
use crate::delivery::LATEST_VALUES;
use crate::latest::LatestValues;

pub fn spawn_latest_projector(bus: &Arc<EventBus>, values: LatestValues) {
    let mut subscription = bus.subscribe_critical(LATEST_VALUES);
    tokio::spawn(async move {
        while let Some(event) = subscription.recv().await {
            values.project(&event).await;
        }
    });
}
