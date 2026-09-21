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
use crate::payload_fields::ad_break as ad_break_fields;

pub(crate) struct AdBreakStartedDescriptor;

impl TriggerKindDescriptor for AdBreakStartedDescriptor {
    fn id(&self) -> &str {
        "twitch.stream.ad_break_started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Ad break started"
    }

    fn summary(&self) -> &str {
        "Fires when an ad break begins on the broadcaster's channel"
    }

    fn search_text(&self) -> &str {
        "twitch ad break commercial started begun automatic"
    }

    fn icon_name(&self) -> &str {
        "broadcast"
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
            kind_prefix: Some("twitch.channel.ad_break.begin".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), requester_identity)
                .event_specific(
                    DeclaredVariable {
                        name: "ad_break.duration_seconds".to_owned(),
                        kind: VariantKind::Int,
                        label: "Ad break duration (seconds)".to_owned(),
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
                        label: "Automatic ad break".to_owned(),
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
                .event_specific(
                    DeclaredVariable {
                        name: "ad_break.started_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Ad break start time".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            ad_break_fields::AD_BREAK,
                            ad_break_fields::STARTED_AT,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "requester_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Requester login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            ad_break_fields::REQUESTER,
                            ad_break_fields::REQUESTER_LOGIN,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

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

    fn ad_break_event() -> Event {
        let payload = serde_json::json!({
            "ad_break": {
                "duration_seconds": 90,
                "is_automatic": true,
                "started_at": "2026-06-13T10:00:00Z",
            },
            "requester": { "login": "broadcaster_one" },
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.ad_break.begin",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_ad_break_begin_topic_from_twitch() {
        let filter = AdBreakStartedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.ad_break.begin")
        );
    }

    #[test]
    fn build_arg_stack_types_duration_as_int_and_is_automatic_as_bool() {
        let stack = AdBreakStartedDescriptor.build_arg_stack(&ad_break_event());
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
            Some(&Variant::String("2026-06-13T10:00:00Z".to_owned()))
        );
        assert_eq!(
            stack.get("requester_login"),
            Some(&Variant::String("broadcaster_one".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_defaults_missing_numeric_and_bool_fields() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.ad_break.begin",
            serde_json::json!({}),
        );
        let stack = AdBreakStartedDescriptor.build_arg_stack(&event);
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
