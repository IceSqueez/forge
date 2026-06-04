use forge_events::{Event, EventPublisher, EventSource};
use serde_json::json;

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
pub(crate) fn publish_failed(publisher: &dyn EventPublisher, webhook_name: &str, reason: &str) {
    publisher.publish(Event::new(
        EventSource::Discord,
        "discord.webhook.failed",
        json!({
            "webhook_name": webhook_name,
            "reason":        reason,
        }),
    ));
}

#[allow(dead_code)]
pub(crate) fn publish_ratelimit_hit(
    publisher: &dyn EventPublisher,
    webhook_name: &str,
    retry_after_secs: f64,
) {
    publisher.publish(Event::new(
        EventSource::Discord,
        "discord.webhook.ratelimit.hit",
        json!({
            "webhook_name":      webhook_name,
            "retry_after_secs":  retry_after_secs,
        }),
    ));
}
