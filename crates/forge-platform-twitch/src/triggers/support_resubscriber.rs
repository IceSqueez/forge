use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::ChatEventKind;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, CanonicalCount, CanonicalVariable, TriggerCategory,
    TriggerVariables,
};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::support as fields;
use forge_types::{DeclaredVariable, Variant, VariantKind};

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), resubscriber_identity)
            .message_text(resub_message)
            .count(CanonicalCount::SubCumulativeMonths, |event| {
                payload_read::number(event, fields::CUMULATIVE_MONTHS)
            })
            .count(CanonicalCount::SubStreakMonths, |event| {
                payload_read::number(event, fields::STREAK_MONTHS)
            })
            .sub_tier(|event| payload_read::text(event, fields::TIER))
            .event_specific(
                DeclaredVariable {
                    name: "sub_is_prime".to_owned(),
                    kind: VariantKind::Bool,
                    synthesis: None,
                },
                |event| Variant::Bool(payload_read::flag(event, fields::IS_PRIME)),
            )
            .legacy("sub_message", CanonicalVariable::MessageText),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SUPPORT_RESUBSCRIBER,
    category: TriggerCategory::Subscriptions,
    label: "Re-subscriber",
    summary: "Fires on subscription renewals",
    search_text: "twitch resub resubscribed resubscribe renewal subscription months streak",
    icon: "repeat",
    kind: event_kinds::CHANNEL_SUBSCRIPTION_MESSAGE,
    actors: ActorDeclaration::principal(),
    variables,
    chat_event: Some(ChatEventKind::Resubscription),
    ..TwitchTriggerSpec::BASE
};

fn resubscriber_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::USER),
        fields::USER_ID,
        fields::USER_LOGIN,
        fields::USER_DISPLAY_NAME,
    )
}

fn resub_message(event: &Event) -> String {
    payload_read::text(event, fields::MESSAGE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::TriggerConfig;
    use forge_types::Variant;

    fn resub_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.subscription.message",
            serde_json::json!({
                "user": { "id": "222", "login": "loyalfan", "display_name": "LoyalFan" },
                "tier": "1000",
                "cumulative_months": 12,
                "streak_months": 6,
                "message": "Love this channel!",
                "share_streak": true
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(SPEC.matches_trigger(&TriggerConfig::new(), &resub_event()));
    }

    #[test]
    fn a_resubscriber_publishes_the_canonical_actor_block_the_tier_and_the_resub_message() {
        let stack = SPEC.build_arg_stack(&resub_event());
        for (name, value) in [
            ("user_id", "222"),
            ("user_name", "LoyalFan"),
            ("user_login", "loyalfan"),
            ("user_platform", "twitch"),
            ("sub_tier", "1000"),
            ("message_text", "Love this channel!"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(stack.get("sub_cumulative_months"), Some(&Variant::Int(12)));
        assert_eq!(stack.get("sub_streak_months"), Some(&Variant::Int(6)));
    }

    #[test]
    fn a_resubscriber_exposes_whether_the_resub_was_prime() {
        for (stored, exposed) in [(Some(true), true), (Some(false), false), (None, false)] {
            let mut payload = serde_json::json!({
                "user": { "id": "222", "login": "loyalfan", "display_name": "LoyalFan" },
                "tier": "1000",
                "cumulative_months": 12,
                "streak_months": 6,
                "message": "Love this channel!",
                "share_streak": true
            });
            if let Some(is_prime) = stored {
                payload["is_prime"] = serde_json::json!(is_prime);
            }
            let event = Event::new(EventSource::Twitch, SPEC.kind, payload);

            assert_eq!(
                SPEC.build_arg_stack(&event).get("sub_is_prime"),
                Some(&Variant::Bool(exposed)),
                "stored is_prime {stored:?}"
            );
        }
    }
}
