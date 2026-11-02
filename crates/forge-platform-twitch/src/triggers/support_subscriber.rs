use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::ChatEventKind;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::support as fields;

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), subscriber_identity)
            .sub_tier(|event| payload_read::text(event, fields::TIER))
            .event_specific(
                DeclaredVariable {
                    name: "sub_is_gift".to_owned(),
                    kind: VariantKind::Bool,
                    synthesis: None,
                },
                |event| Variant::Bool(payload_read::flag(event, fields::IS_GIFT)),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SUPPORT_SUBSCRIBER,
    category: TriggerCategory::Subscriptions,
    label: "New subscriber",
    summary: "Fires on a first-time channel subscription",
    search_text: "twitch subscribe subscriber new subscription tier",
    icon: "user-plus",
    kind: event_kinds::CHANNEL_SUBSCRIBE,
    actors: ActorDeclaration::principal(),
    variables,
    chat_event: Some(ChatEventKind::Subscription),
    ..TwitchTriggerSpec::BASE
};

fn subscriber_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::USER),
        fields::USER_ID,
        fields::USER_LOGIN,
        fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::TriggerConfig;

    fn subscribe_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.subscribe",
            serde_json::json!({
                "user": { "id": "111", "login": "newbie", "display_name": "Newbie" },
                "tier": "1000",
                "is_gift": false
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(SPEC.matches_trigger(&TriggerConfig::new(), &subscribe_event()));
    }

    #[test]
    fn a_new_subscriber_publishes_the_canonical_actor_block_and_the_platform_native_tier() {
        let stack = SPEC.build_arg_stack(&subscribe_event());
        for (name, value) in [
            ("user_id", "111"),
            ("user_name", "Newbie"),
            ("user_login", "newbie"),
            ("user_platform", "twitch"),
            ("sub_tier", "1000"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(stack.get("sub_is_gift"), Some(&Variant::Bool(false)));
    }
}
