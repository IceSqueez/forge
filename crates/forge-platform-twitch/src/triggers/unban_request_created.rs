use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, TriggerCategory,
    TriggerVariables,
};
use forge_types::{DeclaredVariable, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::unban_request as unban_request_fields;

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), unban_target_identity)
            .message_text(|event| payload_read::text(event, unban_request_fields::REASON_TEXT))
            .event_specific(
                DeclaredVariable {
                    name: "unban.request_id".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| {
                    Variant::String(payload_read::text(event, unban_request_fields::REQUEST_ID))
                },
            )
            .legacy(
                "unban.target.login",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            )
            .legacy("unban.reason_text", CanonicalVariable::MessageText),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_UNBAN_REQUEST_CREATED,
    category: TriggerCategory::Moderation,
    label: "Unban request created",
    summary: "Fires when a banned viewer submits an unban request",
    search_text: "twitch unban request created submitted appeal moderation",
    icon: "shield-question",
    kind: event_kinds::CHANNEL_UNBAN_REQUEST_CREATE,
    actors: ActorDeclaration::principal(),
    variables,
    ..TwitchTriggerSpec::BASE
};

fn unban_target_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(unban_request_fields::USER),
        unban_request_fields::USER_ID,
        unban_request_fields::USER_LOGIN,
        unban_request_fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

    #[test]
    fn build_arg_stack_maps_create_fields_from_publisher_payload() {
        let payload = serde_json::json!({
            "id": "req-7",
            "user": { "login": "banned_viewer" },
            "reason_text": "please unban me",
        });
        let event = Event::new(
            EventSource::Twitch,
            "twitch.channel.unban_request.create",
            payload,
        );
        let stack = SPEC.build_arg_stack(&event);

        for (key, expected) in [
            ("unban.request_id", "req-7"),
            ("unban.target.login", "banned_viewer"),
            ("unban.reason_text", "please unban me"),
        ] {
            assert_eq!(
                stack.get(key),
                Some(&Variant::String(expected.to_owned())),
                "wrong value for {key}"
            );
        }
    }

    #[test]
    fn build_arg_stack_uses_empty_strings_when_create_payload_is_empty() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.unban_request.create",
            serde_json::json!({}),
        );
        let stack = SPEC.build_arg_stack(&event);

        for key in [
            "unban.request_id",
            "unban.target.login",
            "unban.reason_text",
        ] {
            assert_eq!(
                stack.get(key),
                Some(&Variant::String(String::new())),
                "expected empty string for {key}"
            );
        }
    }
}
