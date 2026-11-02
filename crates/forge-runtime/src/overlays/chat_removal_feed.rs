use std::sync::Arc;

use forge_events::{Delivery, Event};
use forge_net::TaskOwner;
use forge_overlay::RowRemoval;
use forge_types::ChatModerationPayload;

use crate::delivery::CHAT_OVERLAYS;
use crate::event_flow::bus::EventBus;
use crate::overlays::overlay_service::OverlayServiceHandle;

pub fn spawn_chat_removal_feed(
    bus: &Arc<EventBus>,
    overlays: OverlayServiceHandle,
    tasks: &TaskOwner,
) {
    let mut subscription = bus.subscribe_observer(CHAT_OVERLAYS);
    tasks.spawn_until_stopped(async move {
        loop {
            match subscription.next().await {
                Delivery::Event(event) => {
                    if let Some(removal) = removal_of(&event) {
                        overlays.remove_chat_rows(&removal).await;
                    }
                }
                Delivery::Skipped(missed) => {
                    tracing::warn!(missed, "chat overlay removals fell behind the bus");
                }
                Delivery::Closed => break,
            }
        }
    });
}

pub fn removal_of(event: &Event) -> Option<RowRemoval> {
    let platform = event.source.to_platform_id()?;
    let value = event.payload.get(ChatModerationPayload::KEY)?;
    let payload: ChatModerationPayload = serde_json::from_value(value.clone()).ok()?;
    RowRemoval::from_moderation(platform, &payload.action)
}
