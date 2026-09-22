use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalCount, CanonicalVariable, DeclaredVariable, PlatformId,
    SynthesisHint, TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, kick_actor};
use crate::payload_fields::subscription_gift as fields;

pub(crate) struct SubGiftDescriptor;

impl TriggerKindDescriptor for SubGiftDescriptor {
    fn id(&self) -> &str {
        "kick.channel.subscription.gifts"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "Gifted subscriptions"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer gifts subscriptions to the Kick channel"
    }

    fn search_text(&self) -> &str {
        "kick gift sub gifted subscriptions community"
    }

    fn icon_name(&self) -> &str {
        "gift"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
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
            source: Some(EventSource::Kick),
            kind_prefix: Some("kick.channel.subscription.gifts".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(kick_actor(ActorRole::Principal), gifter_identity)
                .actor(kick_actor(ActorRole::Gifter), gifter_identity)
                .actor(kick_actor(ActorRole::Recipient), first_giftee_identity)
                .count(CanonicalCount::GiftCount, |event| {
                    payload_read::number(event, fields::COUNT)
                })
                .sub_tier(|event| payload_read::text(event, fields::TIER))
                .legacy(
                    DeclaredVariable {
                        name: "gifter_username".to_owned(),
                        kind: VariantKind::String,
                        label: "Gifter username".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Gifter, ActorSlot::Login),
                    |event| Variant::String(payload_read::login_of(gifter_identity(event))),
                )
                .legacy(
                    DeclaredVariable {
                        name: "count".to_owned(),
                        kind: VariantKind::Int,
                        label: "Gifted subscription count".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 100 }),
                    },
                    CanonicalVariable::Count(CanonicalCount::GiftCount),
                    |event| Variant::Int(payload_read::number(event, fields::COUNT)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "tier".to_owned(),
                        kind: VariantKind::String,
                        label: "Subscription tier".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::SubTier,
                    |event| Variant::String(payload_read::text(event, fields::TIER)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Gifter, ActorRole::Recipient])
    }
}

fn gifter_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::GIFTER))
}

fn first_giftee_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event
            .payload
            .get(fields::GIFTEES)
            .and_then(serde_json::Value::as_array)
            .and_then(|giftees| giftees.first()),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn gift_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.channel.subscription.gifts",
            serde_json::json!({
                "gifter": { "id": 200, "username": "generous_viewer" },
                "giftees": [
                    { "id": null, "username": "user_a" },
                    { "id": null, "username": "user_b" },
                    { "id": null, "username": "user_c" }
                ],
                "count": 3,
                "tier": "tier1"
            }),
        )
    }

    #[test]
    fn build_arg_stack_extracts_gift_fields() {
        let stack = SubGiftDescriptor.build_arg_stack(&gift_event());
        assert_eq!(
            stack.get("gifter_id"),
            Some(&Variant::String("200".to_owned()))
        );
        assert_eq!(
            stack.get("gifter_username"),
            Some(&Variant::String("generous_viewer".to_owned()))
        );
        assert_eq!(stack.get("count"), Some(&Variant::Int(3)));
        assert_eq!(
            stack.get("tier"),
            Some(&Variant::String("tier1".to_owned()))
        );
    }
}
