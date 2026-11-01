use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, CanonicalCount, TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::support as fields;

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), gifter_identity)
            .actor(twitch_actor(ActorRole::Gifter), gifter_identity)
            .count(CanonicalCount::GiftCount, |event| {
                payload_read::number(event, fields::GIFT_TOTAL)
            })
            .sub_tier(|event| payload_read::text(event, fields::TIER))
            .event_specific(
                DeclaredVariable {
                    name: "gifter_is_anonymous".to_owned(),
                    kind: VariantKind::Bool,
                    label: "Anonymous gifter".to_owned(),
                    synthesis: None,
                },
                |event| Variant::Bool(payload_read::flag(event, fields::IS_ANONYMOUS)),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SUPPORT_GIFT_SUB,
    category: TriggerCategory::Subscriptions,
    label: "Gift subscription",
    summary: "Fires when a user gifts one or more subscriptions",
    search_text: "twitch gift sub gifted subscription gifter",
    icon: "gift",
    kind: event_kinds::CHANNEL_SUBSCRIPTION_GIFT,
    actors: ActorDeclaration::Actors(&[ActorRole::Gifter]),
    variables,
    ..TwitchTriggerSpec::BASE
};

fn gifter_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::GIFTER),
        fields::GIFTER_ID,
        fields::GIFTER_LOGIN,
        fields::GIFTER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::TriggerConfig;

    fn gift_event(gift_total: serde_json::Value) -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.subscription.gift",
            serde_json::json!({
                "tier": "1000",
                "is_anonymous": false,
                "gift_total": gift_total,
                "gifter": {
                    "id": "333",
                    "login": "generous_viewer",
                    "display_name": "GenerousViewer",
                },
            }),
        )
    }

    fn as_twitch_sends_it() -> Event {
        gift_event(serde_json::json!(5))
    }

    #[test]
    fn always_matches() {
        assert!(SPEC.matches_trigger(&TriggerConfig::new(), &as_twitch_sends_it()));
    }

    #[test]
    fn a_gift_sub_publishes_the_gifter_as_both_the_principal_and_the_gifter_role() {
        let stack = SPEC.build_arg_stack(&as_twitch_sends_it());
        for (name, value) in [
            ("user_id", "333"),
            ("user_name", "GenerousViewer"),
            ("user_login", "generous_viewer"),
            ("user_platform", "twitch"),
            ("gifter_id", "333"),
            ("gifter_name", "GenerousViewer"),
            ("gifter_login", "generous_viewer"),
            ("gifter_platform", "twitch"),
            ("sub_tier", "1000"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(
            stack.get("gifter_is_anonymous"),
            Some(&Variant::Bool(false))
        );
    }

    #[test]
    fn a_gift_sub_counts_the_gifts_the_wire_reports_and_counts_a_silent_wire_as_one() {
        for (wire_total, expected) in [
            (serde_json::json!(5), 5),
            (serde_json::json!(1), 1),
            (serde_json::json!(null), 1),
        ] {
            let event = gift_event(wire_total.clone());
            assert_eq!(
                SPEC.build_arg_stack(&event).get("gift_count"),
                Some(&Variant::Int(expected)),
                "wire total {wire_total}"
            );
        }
    }

    #[test]
    fn a_gift_sub_declares_the_gifter_and_the_total_but_no_recipient_variables() {
        let declared: Vec<String> = SPEC
            .variables()
            .unwrap()
            .declarations()
            .into_iter()
            .map(|variable| variable.declared.name)
            .collect();
        for name in ["gifter_id", "gifter_name", "gift_count"] {
            assert!(declared.iter().any(|d| d == name), "missing '{name}'");
        }
        assert!(
            declared.iter().all(|d| !d.starts_with("recipient_")),
            "{declared:?}"
        );
        let stack = SPEC.build_arg_stack(&as_twitch_sends_it());
        assert_eq!(stack.get("recipient_id"), None);
    }
}
