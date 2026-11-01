use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::suspicious as suspicious_fields;

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), suspicious_user_identity)
            .message_text(|event| payload_read::text(event, suspicious_fields::MESSAGE_TEXT))
            .event_specific(
                DeclaredVariable {
                    name: "low_trust_status".to_owned(),
                    kind: VariantKind::String,
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

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_SUSPICIOUS_USER_MESSAGE,
    category: TriggerCategory::Moderation,
    label: "Suspicious user message",
    summary: "Fires when a user flagged as suspicious sends a message in the channel",
    search_text: "twitch suspicious user low trust message moderation ban",
    icon: "alert-triangle",
    kind: event_kinds::CHANNEL_SUSPICIOUS_USER_MESSAGE,
    actors: ActorDeclaration::principal(),
    variables,
    ..TwitchTriggerSpec::BASE
};

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
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

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
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.suspicious_user.message")
        );
    }

    #[test]
    fn build_arg_stack_maps_user_low_trust_and_message_fields() {
        let stack = SPEC.build_arg_stack(&suspicious_user_event());
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
