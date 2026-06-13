use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct SuspiciousUserMessageDescriptor;

impl TriggerKindDescriptor for SuspiciousUserMessageDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.suspicious_user_message"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Suspicious user message"
    }

    fn summary(&self) -> &str {
        "Fires when a user flagged as suspicious sends a message in the channel"
    }

    fn search_text(&self) -> &str {
        "twitch suspicious user low trust message moderation ban"
    }

    fn icon_name(&self) -> &str {
        "alert-triangle"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.suspicious_user.message".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user = event.payload.get("user");

        let user_login = user
            .and_then(|v| v.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = user
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let low_trust_status = event
            .payload
            .get("low_trust_status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let message_text = event
            .payload
            .get("message_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set(
                "low_trust_status".to_owned(),
                Variant::String(low_trust_status),
            )
            .set("message_text".to_owned(), Variant::String(message_text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suspicious_user_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "321", "login": "shady_one", "display_name": "ShadyOne" },
            "low_trust_status": "active_monitoring",
            "message_text": "is this a scam link",
        });
        Event::new(
            EventSource::Twitch,
            "channel.suspicious_user.message",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_suspicious_user_message_topic_from_twitch() {
        let filter = SuspiciousUserMessageDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.suspicious_user.message")
        );
    }

    #[test]
    fn build_arg_stack_maps_user_low_trust_and_message_fields() {
        let stack = SuspiciousUserMessageDescriptor.build_arg_stack(&suspicious_user_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("shady_one".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("321".to_owned()))
        );
        assert_eq!(
            stack.get("low_trust_status"),
            Some(&Variant::String("active_monitoring".to_owned()))
        );
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("is this a scam link".to_owned()))
        );
    }
}
