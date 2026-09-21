use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalCount, CanonicalVariable, DeclaredVariable, PlatformId,
    SynthesisHint, TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::shoutout as shoutout_fields;

pub(crate) struct ShoutoutSentDescriptor;

impl TriggerKindDescriptor for ShoutoutSentDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.shoutout_sent"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Shoutout sent"
    }

    fn summary(&self) -> &str {
        "Fires when the broadcaster or a moderator gives a shoutout to another channel"
    }

    fn search_text(&self) -> &str {
        "twitch shoutout sent give raid moderation"
    }

    fn icon_name(&self) -> &str {
        "megaphone"
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
            kind_prefix: Some("twitch.channel.shoutout.create".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(
                    twitch_actor(ActorRole::Principal),
                    shoutout_channel_identity,
                )
                .count(CanonicalCount::ViewerCount, |event| {
                    payload_read::number(event, shoutout_fields::VIEWER_COUNT)
                })
                .event_specific(
                    DeclaredVariable {
                        name: "started_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Started at".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, shoutout_fields::STARTED_AT)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "to_broadcaster_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Target channel login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            shoutout_fields::TO_BROADCASTER,
                            shoutout_fields::BROADCASTER_LOGIN,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "to_broadcaster_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Target channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            shoutout_fields::TO_BROADCASTER,
                            shoutout_fields::BROADCASTER_ID,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn shoutout_channel_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(shoutout_fields::TO_BROADCASTER),
        shoutout_fields::BROADCASTER_ID,
        shoutout_fields::BROADCASTER_LOGIN,
        shoutout_fields::BROADCASTER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shoutout_sent_event() -> Event {
        let payload = serde_json::json!({
            "to_broadcaster": { "id": "555", "login": "other_chan", "display_name": "OtherChan" },
            "viewer_count": 42,
            "started_at": "2026-06-13T18:00:00Z",
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.shoutout.create",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_shoutout_create_topic_from_twitch() {
        let filter = ShoutoutSentDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.shoutout.create")
        );
    }

    #[test]
    fn build_arg_stack_maps_target_fields_and_types_viewer_count_as_int() {
        let stack = ShoutoutSentDescriptor.build_arg_stack(&shoutout_sent_event());
        assert_eq!(
            stack.get("to_broadcaster_login"),
            Some(&Variant::String("other_chan".to_owned()))
        );
        assert_eq!(
            stack.get("to_broadcaster_id"),
            Some(&Variant::String("555".to_owned()))
        );
        assert_eq!(stack.get("viewer_count"), Some(&Variant::Int(42)));
        assert_eq!(
            stack.get("started_at"),
            Some(&Variant::String("2026-06-13T18:00:00Z".to_owned()))
        );
    }
}
