use std::sync::Arc;

use forge_events::Event;
use forge_types::{ChatModerationAction, ChatModerationPayload, ChatSource};
use futures_core::Stream;
use futures_util::stream;

use crate::bus::EventBus;
use crate::chat_stream::event_source_to_chat_source;
use crate::delivery::CHAT_MODERATION;

pub fn chat_moderation_stream(
    bus: Arc<EventBus>,
) -> impl Stream<Item = (ChatSource, ChatModerationAction)> + Send + 'static {
    let subscription = bus.subscribe_critical(CHAT_MODERATION);
    stream::unfold(subscription, |mut subscription| async move {
        while let Some(event) = subscription.recv().await {
            if let Some(item) = try_map_moderation_event(&event) {
                return Some((item, subscription));
            }
        }
        None
    })
}

fn try_map_moderation_event(ev: &Event) -> Option<(ChatSource, ChatModerationAction)> {
    let source = event_source_to_chat_source(ev.source)?;
    let value = ev.payload.get(ChatModerationPayload::KEY)?;
    let payload: ChatModerationPayload = serde_json::from_value(value.clone()).ok()?;
    Some((source, payload.action))
}
