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
use crate::payload_fields::unban_request as unban_request_fields;

pub(crate) struct UnbanRequestCreatedDescriptor;

impl TriggerKindDescriptor for UnbanRequestCreatedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.unban_request_created"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Unban request created"
    }

    fn summary(&self) -> &str {
        "Fires when a banned viewer submits an unban request"
    }

    fn search_text(&self) -> &str {
        "twitch unban request created submitted appeal moderation"
    }

    fn icon_name(&self) -> &str {
        "shield-question"
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
            kind_prefix: Some("twitch.channel.unban_request.create".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), unban_target_identity)
                .message_text(|event| payload_read::text(event, unban_request_fields::REASON_TEXT))
                .event_specific(
                    DeclaredVariable {
                        name: "unban.request_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Unban request ID".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::text(event, unban_request_fields::REQUEST_ID))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "unban.target.login".to_owned(),
                        kind: VariantKind::String,
                        label: "Target user login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            unban_request_fields::USER,
                            unban_request_fields::USER_LOGIN,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "unban.reason_text".to_owned(),
                        kind: VariantKind::String,
                        label: "Requester reason".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    CanonicalVariable::MessageText,
                    |event| {
                        Variant::String(payload_read::text(
                            event,
                            unban_request_fields::REASON_TEXT,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

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

    #[test]
    fn event_filter_targets_unban_request_create_topic_from_twitch() {
        let filter = UnbanRequestCreatedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.unban_request.create")
        );
    }

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
        let stack = UnbanRequestCreatedDescriptor.build_arg_stack(&event);

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
        let stack = UnbanRequestCreatedDescriptor.build_arg_stack(&event);

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
