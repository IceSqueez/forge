use forge_events::{Event, EventPublisher, EventSource};
use serde_json::json;

use crate::error::DiscordError;
use crate::vocabulary::event_kinds;
use crate::vocabulary::fields::{failed, posted, rate_limited, webhook};

pub(crate) fn publish_posted(
    publisher: &dyn EventPublisher,
    webhook_name: &str,
    message_id: &str,
    embed_count: u8,
) {
    publisher.publish(Event::new(
        EventSource::Discord,
        event_kinds::WEBHOOK_POSTED,
        json!({
            (webhook::WEBHOOK_NAME): webhook_name,
            (posted::MESSAGE_ID): message_id,
            (posted::EMBED_COUNT): embed_count,
        }),
    ));
}

pub(crate) fn publish_failed(
    publisher: &dyn EventPublisher,
    webhook_name: &str,
    err: &DiscordError,
) {
    publisher.publish(Event::new(
        EventSource::Discord,
        event_kinds::WEBHOOK_FAILED,
        json!({
            (webhook::WEBHOOK_NAME): webhook_name,
            (failed::REASON): err.reason_token(),
            (failed::DETAIL): err.detail(),
            (failed::STATUS_CODE): err.status_code(),
        }),
    ));
}

pub(crate) fn publish_rate_limited(
    publisher: &dyn EventPublisher,
    webhook_name: &str,
    retry_after_secs: f64,
) {
    publisher.publish(Event::new(
        EventSource::Discord,
        event_kinds::WEBHOOK_RATE_LIMITED,
        json!({
            (webhook::WEBHOOK_NAME): webhook_name,
            (rate_limited::RETRY_AFTER_SECS): retry_after_secs,
        }),
    ));
}
