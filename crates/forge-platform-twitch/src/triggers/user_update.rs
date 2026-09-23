use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::user as user_fields;

pub(crate) struct UserUpdateDescriptor;

impl TriggerKindDescriptor for UserUpdateDescriptor {
    fn id(&self) -> &str {
        "twitch.user.update"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Users
    }

    fn label(&self) -> &str {
        "User profile updated"
    }

    fn summary(&self) -> &str {
        "Fires when the authenticated user changes their profile description or display name"
    }

    fn search_text(&self) -> &str {
        "twitch user update profile description display name account changed"
    }

    fn icon_name(&self) -> &str {
        "user-pen"
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
        "any profile change".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("twitch.user.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
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
                    DeclaredVariable {
                        name: "user.id".to_owned(),
                        kind: VariantKind::String,
                        label: "User ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            user_fields::USER,
                            user_fields::USER_ID,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "user.login".to_owned(),
                        kind: VariantKind::String,
                        label: "User login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            user_fields::USER,
                            user_fields::USER_LOGIN,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "user.display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "User display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            user_fields::USER,
                            user_fields::USER_DISPLAY_NAME,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn updated_user_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(user_fields::USER),
        user_fields::USER_ID,
        user_fields::USER_LOGIN,
        user_fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
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

        let stack = UserUpdateDescriptor.build_arg_stack(&event);

        expect_string(&stack, "user.id", "1337");
        expect_string(&stack, "user.login", "thestreamer");
        expect_string(&stack, "user.display_name", "TheStreamer");
        expect_string(&stack, "user.description", "now playing forge");
    }

    #[test]
    fn build_arg_stack_on_empty_payload_yields_blank_fields_without_panic() {
        let stack = UserUpdateDescriptor.build_arg_stack(&event_with(json!({})));

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

        let stack = UserUpdateDescriptor.build_arg_stack(&event);

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
        let filter = UserUpdateDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.user.update"));
    }
}
