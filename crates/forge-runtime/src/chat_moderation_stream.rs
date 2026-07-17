use std::sync::Arc;

use forge_events::Event;
use forge_types::{ChatModerationAction, ChatModerationPayload, ChatSource};
use futures_core::Stream;
use futures_util::stream;
use tokio::sync::broadcast;

use crate::bus::EventBus;
use crate::chat_stream::event_source_to_chat_source;

pub fn chat_moderation_stream(
    bus: Arc<EventBus>,
) -> impl Stream<Item = (ChatSource, ChatModerationAction)> + Send + 'static {
    let receiver = bus.subscribe().into_receiver();
    stream::unfold(receiver, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    if let Some(item) = try_map_moderation_event(ev) {
                        return Some((item, rx));
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(missed = n, "chat moderation stream subscriber lagged");
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    })
}

fn try_map_moderation_event(ev: Event) -> Option<(ChatSource, ChatModerationAction)> {
    let source = event_source_to_chat_source(ev.source)?;
    let value = ev.payload.get(ChatModerationPayload::KEY)?;
    let payload: ChatModerationPayload = serde_json::from_value(value.clone()).ok()?;
    Some((source, payload.action))
}
