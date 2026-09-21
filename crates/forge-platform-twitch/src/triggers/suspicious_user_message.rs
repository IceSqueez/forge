use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::suspicious as suspicious_fields;

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
            kind_prefix: Some("twitch.channel.suspicious_user.message".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), suspicious_user_identity)
                .message_text(|event| payload_read::text(event, suspicious_fields::MESSAGE_TEXT))
                .event_specific(
                    DeclaredVariable {
                        name: "low_trust_status".to_owned(),
                        kind: VariantKind::String,
                        label: "Low-trust status".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::text(
                            event,
                            suspicious_fields::LOW_TRUST_STATUS,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn suspicious_user_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(suspicious_fields::USER),
        suspicious_fields::USER_ID,
        suspicious_fields::USER_LOGIN,
        suspicious_fields::USER_DISPLAY_NAME,
    )
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
            Some("twitch.channel.suspicious_user.message")
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
