use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, TriggerCategory,
    TriggerVariables,
};
use forge_types::{DeclaredVariable, SynthesisHint, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::ad_break as ad_break_fields;

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), requester_identity)
            .event_specific(
                DeclaredVariable {
                    name: "ad_break.duration_seconds".to_owned(),
                    kind: VariantKind::Int,
                    synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 180 }),
                },
                |event| {
                    Variant::Int(payload_read::nested_number(
                        event,
                        ad_break_fields::AD_BREAK,
                        ad_break_fields::DURATION_SECONDS,
                    ))
                },
            )
            .event_specific(
                DeclaredVariable {
                    name: "ad_break.is_automatic".to_owned(),
                    kind: VariantKind::Bool,
                    synthesis: None,
                },
                |event| {
                    Variant::Bool(payload_read::nested_flag(
                        event,
                        ad_break_fields::AD_BREAK,
                        ad_break_fields::IS_AUTOMATIC,
                    ))
                },
            )
            .event_specific_optional(
                DeclaredVariable {
                    name: "ad_break.started_at".to_owned(),
                    kind: VariantKind::Datetime,
                    synthesis: None,
                },
                |event| {
                    payload_read::nested_timestamp(
                        event,
                        ad_break_fields::AD_BREAK,
                        ad_break_fields::STARTED_AT,
                    )
                },
            )
            .legacy(
                "requester_login",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::STREAM_AD_BREAK_STARTED,
    category: TriggerCategory::Streams,
    label: "Ad break started",
    summary: "Fires when an ad break begins on the broadcaster's channel",
    search_text: "twitch ad break commercial started begun automatic",
    icon: "broadcast",
    kind: event_kinds::CHANNEL_AD_BREAK_BEGIN,
    actors: ActorDeclaration::principal(),
    variables,
    ..TwitchTriggerSpec::BASE
};

fn requester_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(ad_break_fields::REQUESTER),
        ad_break_fields::REQUESTER_ID,
        ad_break_fields::REQUESTER_LOGIN,
        ad_break_fields::REQUESTER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp::tests::instant;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

    fn ad_break_event() -> Event {
        let payload = serde_json::json!({
            "ad_break": {
                "duration_seconds": 90,
                "is_automatic": true,
                "started_at": "2026-06-13T10:00:00Z",
            },
            "requester": {
                "id": "1",
                "login": "broadcaster_one",
                "display_name": "BroadcasterOne",
            },
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.ad_break.begin",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_ad_break_begin_topic_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.ad_break.begin")
        );
    }

    #[test]
    fn an_ad_break_publishes_whoever_requested_it_as_the_canonical_actor() {
        let stack = SPEC.build_arg_stack(&ad_break_event());
        for (name, value) in [
            ("user_id", "1"),
            ("user_name", "BroadcasterOne"),
            ("user_login", "broadcaster_one"),
            ("user_platform", "twitch"),
            ("requester_login", "broadcaster_one"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
    }

    #[test]
    fn build_arg_stack_types_duration_as_int_and_is_automatic_as_bool() {
        let stack = SPEC.build_arg_stack(&ad_break_event());
        assert_eq!(
            stack.get("ad_break.duration_seconds"),
            Some(&Variant::Int(90))
        );
        assert_eq!(
            stack.get("ad_break.is_automatic"),
            Some(&Variant::Bool(true))
        );
        assert_eq!(
            stack.get("ad_break.started_at"),
            Some(&instant("2026-06-13T10:00:00Z"))
        );
    }

    #[test]
    fn build_arg_stack_defaults_missing_numeric_and_bool_fields() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.ad_break.begin",
            serde_json::json!({}),
        );
        let stack = SPEC.build_arg_stack(&event);
        assert_eq!(
            stack.get("ad_break.duration_seconds"),
            Some(&Variant::Int(0))
        );
        assert_eq!(
            stack.get("ad_break.is_automatic"),
            Some(&Variant::Bool(false))
        );
    }
}
