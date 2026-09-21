use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, CanonicalCount, DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::support as fields;

pub(crate) struct SupportGiftSubDescriptor;

impl TriggerKindDescriptor for SupportGiftSubDescriptor {
    fn id(&self) -> &str {
        "twitch.support.gift_sub"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "Gift subscription"
    }

    fn summary(&self) -> &str {
        "Fires when a subscription is gifted to another user"
    }

    fn search_text(&self) -> &str {
        "twitch gift sub gifted subscription recipient"
    }

    fn icon_name(&self) -> &str {
        "gift"
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
            kind_prefix: Some("twitch.channel.subscription.gift".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), gifter_identity)
                .actor(twitch_actor(ActorRole::Gifter), gifter_identity)
                .actor(twitch_actor(ActorRole::Recipient), recipient_identity)
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

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Gifter, ActorRole::Recipient])
    }
}

fn gifter_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::GIFTER),
        fields::GIFTER_ID,
        fields::GIFTER_LOGIN,
        fields::GIFTER_DISPLAY_NAME,
    )
}

fn recipient_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::RECIPIENT),
        fields::RECIPIENT_ID,
        fields::RECIPIENT_LOGIN,
        fields::RECIPIENT_DISPLAY_NAME,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn gift_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.subscription.gift",
            serde_json::json!({
                "tier": "1000",
                "is_anonymous": false,
                "gifter": { "id": "333", "login": "generous_viewer", "display_name": "GenerousViewer", "total": 5 },
                "recipient": { "id": "444", "login": "lucky_one", "display_name": "LuckyOne" }
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(SupportGiftSubDescriptor.matches_trigger(&TriggerConfig::new(), &gift_event()));
    }

    #[test]
    fn build_arg_stack_extracts_gift_fields() {
        let stack = SupportGiftSubDescriptor.build_arg_stack(&gift_event());
        assert_eq!(
            stack.get("gifter_login"),
            Some(&Variant::String("generous_viewer".to_owned()))
        );
        assert_eq!(
            stack.get("gifter_is_anonymous"),
            Some(&Variant::Bool(false))
        );
        assert_eq!(
            stack.get("recipient_login"),
            Some(&Variant::String("lucky_one".to_owned()))
        );
        assert_eq!(
            stack.get("sub_tier"),
            Some(&Variant::String("1000".to_owned()))
        );
    }
}
