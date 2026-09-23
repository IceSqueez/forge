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
use crate::payload_fields::subscription as fields;

pub(crate) struct SubDescriptor;

impl TriggerKindDescriptor for SubDescriptor {
    fn id(&self) -> &str {
        "kick.channel.subscribed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "Subscription"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer subscribes to the Kick channel"
    }

    fn search_text(&self) -> &str {
        "kick subscription new sub supporter tier"
    }

    fn icon_name(&self) -> &str {
        "star"
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
            kind_prefix: Some("kick.channel.subscribed".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(kick_actor(ActorRole::Principal), subscriber_identity)
                .count(CanonicalCount::SubCumulativeMonths, |event| {
                    payload_read::number(event, fields::MONTHS)
                })
                .sub_tier(|event| payload_read::text(event, fields::TIER))
                .legacy(
                    DeclaredVariable {
                        name: "username".to_owned(),
                        kind: VariantKind::String,
                        label: "Subscriber username".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| Variant::String(payload_read::login_of(subscriber_identity(event))),
                )
                .legacy(
                    DeclaredVariable {
                        name: "months".to_owned(),
                        kind: VariantKind::Int,
                        label: "Subscribed months".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 1, max: 24 }),
                    },
                    CanonicalVariable::Count(CanonicalCount::SubCumulativeMonths),
                    |event| Variant::Int(payload_read::number(event, fields::MONTHS)),
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
        ActorDeclaration::principal()
    }
}

fn subscriber_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::SUBSCRIBER))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use serde_json::json;

    fn a_third_month_subscription() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.channel.subscribed",
            json!({
                "subscriber": { "id": 123, "username": "new_subscriber" },
                "months": 3,
                "tier": "tier1"
            }),
        )
    }

    #[test]
    fn a_subscription_publishes_the_subscriber_beside_the_canonical_tier_and_month_count() {
        let stack = SubDescriptor.build_arg_stack(&a_third_month_subscription());
        for (name, value) in [
            ("user_id", "123"),
            ("user_login", "new_subscriber"),
            ("sub_tier", "tier1"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(
            stack.get("sub_cumulative_months"),
            Some(&Variant::Int(3)),
            "sub_cumulative_months"
        );
    }

    #[test]
    fn the_legacy_sub_names_still_carry_what_their_canonical_twins_carry() {
        let stack = SubDescriptor.build_arg_stack(&a_third_month_subscription());
        assert_eq!(stack.get("username"), stack.get("user_login"));
        assert_eq!(stack.get("months"), stack.get("sub_cumulative_months"));
        assert_eq!(stack.get("tier"), stack.get("sub_tier"));
        assert_eq!(
            stack.get("username"),
            Some(&Variant::String("new_subscriber".to_owned()))
        );
        assert_eq!(stack.get("months"), Some(&Variant::Int(3)));
    }
}
