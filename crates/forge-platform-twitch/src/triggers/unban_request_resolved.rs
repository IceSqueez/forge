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

pub(crate) struct UnbanRequestResolvedDescriptor;

impl TriggerKindDescriptor for UnbanRequestResolvedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.unban_request_resolved"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Unban request resolved"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator approves, denies, or cancels an unban request"
    }

    fn search_text(&self) -> &str {
        "twitch unban request resolved approved denied canceled moderator moderation"
    }

    fn icon_name(&self) -> &str {
        "shield-check"
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
            kind_prefix: Some("twitch.channel.unban_request.resolve".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), unban_target_identity)
                .actor(
                    twitch_actor(ActorRole::Moderator),
                    resolving_moderator_identity,
                )
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
                .event_specific(
                    DeclaredVariable {
                        name: "unban.resolution".to_owned(),
                        kind: VariantKind::String,
                        label: "Resolution status".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::text(event, unban_request_fields::STATUS))
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
                        name: "unban.moderator.login".to_owned(),
                        kind: VariantKind::String,
                        label: "Resolving moderator login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Moderator, ActorSlot::Login),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            unban_request_fields::MODERATOR,
                            unban_request_fields::MODERATOR_LOGIN,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "unban.resolution_text".to_owned(),
                        kind: VariantKind::String,
                        label: "Resolution note".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    |event| {
                        Variant::String(payload_read::text(
                            event,
                            unban_request_fields::RESOLUTION_TEXT,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Moderator])
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

fn resolving_moderator_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(unban_request_fields::MODERATOR),
        unban_request_fields::MODERATOR_ID,
        unban_request_fields::MODERATOR_LOGIN,
        unban_request_fields::MODERATOR_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_filter_targets_unban_request_resolve_topic_from_twitch() {
        let filter = UnbanRequestResolvedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.unban_request.resolve")
        );
    }

    #[test]
    fn build_arg_stack_maps_all_resolve_fields_from_publisher_payload() {
        let payload = serde_json::json!({
            "id": "req-42",
            "user": { "login": "banned_viewer" },
            "status": "approved",
            "moderator": { "login": "mod_alice" },
            "resolution_text": "appeal accepted",
        });
        let event = Event::new(
            EventSource::Twitch,
            "channel.unban_request.resolve",
            payload,
        );
        let stack = UnbanRequestResolvedDescriptor.build_arg_stack(&event);

        for (key, expected) in [
            ("unban.request_id", "req-42"),
            ("unban.target.login", "banned_viewer"),
            ("unban.resolution", "approved"),
            ("unban.moderator.login", "mod_alice"),
            ("unban.resolution_text", "appeal accepted"),
        ] {
            assert_eq!(
                stack.get(key),
                Some(&Variant::String(expected.to_owned())),
                "wrong value for {key}"
            );
        }
    }

    #[test]
    fn build_arg_stack_uses_empty_strings_when_resolve_payload_is_empty() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.unban_request.resolve",
            serde_json::json!({}),
        );
        let stack = UnbanRequestResolvedDescriptor.build_arg_stack(&event);

        for key in [
            "unban.request_id",
            "unban.target.login",
            "unban.resolution",
            "unban.moderator.login",
            "unban.resolution_text",
        ] {
            assert_eq!(
                stack.get(key),
                Some(&Variant::String(String::new())),
                "expected empty string for {key}"
            );
        }
    }
}
