use forge_events::{Event, EventPublisher, EventSource};
use serde_json::json;

use crate::error::DiscordError;

#[allow(dead_code)]
pub(crate) fn publish_posted(
    publisher: &dyn EventPublisher,
    webhook_name: &str,
    message_id: &str,
    embed_count: u8,
) {
    publisher.publish(Event::new(
        EventSource::Discord,
        "discord.webhook.posted",
        json!({
            "webhook_name": webhook_name,
            "message_id":   message_id,
            "embed_count":  embed_count,
        }),
    ));
}

#[allow(dead_code)]
pub(crate) fn publish_failed(
    publisher: &dyn EventPublisher,
    webhook_name: &str,
    err: &DiscordError,
) {
    publisher.publish(Event::new(
        EventSource::Discord,
        "discord.webhook.failed",
        json!({
            "webhook_name": webhook_name,
            "reason":       err.reason_token(),
            "detail":       err.detail(),
            "status_code":  err.status_code(),
        }),
    ));
}

#[allow(dead_code)]
pub(crate) fn publish_rate_limited(
    publisher: &dyn EventPublisher,
    webhook_name: &str,
    retry_after_secs: f64,
) {
    publisher.publish(Event::new(
        EventSource::Discord,
        "discord.webhook.rate_limited",
        json!({
            "webhook_name":      webhook_name,
            "retry_after_secs":  retry_after_secs,
        }),
    ));
}
