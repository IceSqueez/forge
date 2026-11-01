use super::spec::TwitchTriggerSpec;
use crate::vocabulary::trigger_ids;
use crate::vocabulary::{event_kinds, messages};
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, TriggerCategory,
    TriggerVariables,
};
use forge_types::{DeclaredVariable, TriggerConfig, UserText, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::user as user_fields;

fn condition_display(_: &TriggerConfig) -> UserText {
    messages::CONDITION_ANY_PROFILE_CHANGE.into()
}

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), updated_user_identity)
            .event_specific(
                DeclaredVariable {
                    name: "user.description".to_owned(),
                    kind: VariantKind::String,
                    label: "User bio".to_owned(),
                    synthesis: None,
                },
                |event| {
                    Variant::String(payload_read::nested_text(
                        event,
                        user_fields::USER,
                        user_fields::USER_DESCRIPTION,
                    ))
                },
            )
            .legacy(
                "user.id",
                "User ID",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
            )
            .legacy(
                "user.login",
                "User login",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            )
            .legacy(
                "user.display_name",
                "User display name",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::USER_UPDATE,
    category: TriggerCategory::Users,
    label: "User profile updated",
    summary: "Fires when the authenticated user changes their profile description or display name",
    search_text: "twitch user update profile description display name account changed",
    icon: "user-pen",
    kind: event_kinds::USER_UPDATE,
    actors: ActorDeclaration::principal(),
    variables,
    condition_display,
    ..TwitchTriggerSpec::BASE
};

fn updated_user_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(user_fields::USER),
        user_fields::USER_ID,
        user_fields::USER_LOGIN,
        user_fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::ArgStack;
    use serde_json::json;

    fn event_with(payload: serde_json::Value) -> Event {
        Event::new(EventSource::Twitch, "twitch.user.update", payload)
    }

    fn expect_string(stack: &ArgStack, key: &str, expected: &str) {
        assert_eq!(
            stack.get(key),
            Some(&Variant::String(expected.to_owned())),
            "arg {key}"
        );
    }

    #[test]
    fn build_arg_stack_surfaces_every_user_field_from_payload() {
        let event = event_with(json!({
            "user": {
                "id": "1337",
                "login": "thestreamer",
                "display_name": "TheStreamer",
                "description": "now playing forge",
            },
        }));

        let stack = SPEC.build_arg_stack(&event);

        expect_string(&stack, "user.id", "1337");
        expect_string(&stack, "user.login", "thestreamer");
        expect_string(&stack, "user.display_name", "TheStreamer");
        expect_string(&stack, "user.description", "now playing forge");
    }

    #[test]
    fn build_arg_stack_on_empty_payload_yields_blank_fields_without_panic() {
        let stack = SPEC.build_arg_stack(&event_with(json!({})));

        for key in [
            "user.id",
            "user.login",
            "user.display_name",
            "user.description",
        ] {
            expect_string(&stack, key, "");
        }
    }

    #[test]
    fn build_arg_stack_never_surfaces_email_pii_even_when_present_in_payload() {
        let secret_email = "streamer@example.com";
        let event = event_with(json!({
            "user": {
                "id": "1337",
                "login": "thestreamer",
                "display_name": "TheStreamer",
                "description": "bio",
                "email": secret_email,
                "email_verified": true,
            },
        }));

        let stack = SPEC.build_arg_stack(&event);

        for (key, value) in stack.snapshot() {
            assert!(
                !key.contains("email"),
                "ArgStack exposed an email-named arg: {key}"
            );
            if let Variant::String(s) = &value {
                assert_ne!(s, secret_email, "ArgStack leaked the email value at {key}");
            }
        }
    }

    #[test]
    fn event_filter_targets_twitch_user_update_kind() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.user.update"));
    }
}
