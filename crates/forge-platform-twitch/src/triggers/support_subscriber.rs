use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::support as fields;

pub(crate) struct SupportSubscriberDescriptor;

impl TriggerKindDescriptor for SupportSubscriberDescriptor {
    fn id(&self) -> &str {
        "twitch.support.subscriber"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "New subscriber"
    }

    fn summary(&self) -> &str {
        "Fires on a first-time channel subscription"
    }

    fn search_text(&self) -> &str {
        "twitch subscribe subscriber new subscription tier"
    }

    fn icon_name(&self) -> &str {
        "user-plus"
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
            kind_prefix: Some("twitch.channel.subscribe".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), subscriber_identity)
                .sub_tier(|event| payload_read::text(event, fields::TIER))
                .event_specific(
                    DeclaredVariable {
                        name: "sub_is_gift".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Gifted subscription".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::Bool(payload_read::flag(event, fields::IS_GIFT)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn subscriber_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::USER),
        fields::USER_ID,
        fields::USER_LOGIN,
        fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
        assert!(
            SupportSubscriberDescriptor.matches_trigger(&TriggerConfig::new(), &subscribe_event())
        );
    }

    #[test]
    fn a_new_subscriber_publishes_the_canonical_actor_block_and_the_platform_native_tier() {
        let stack = SupportSubscriberDescriptor.build_arg_stack(&subscribe_event());
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
