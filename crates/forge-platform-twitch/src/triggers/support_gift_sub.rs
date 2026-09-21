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

    fn gift_event(gift_total: serde_json::Value, recipient: serde_json::Value) -> Event {
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
                "recipient": recipient,
            }),
        )
    }

    fn as_twitch_sends_it() -> Event {
        gift_event(
            serde_json::json!(5),
            serde_json::json!({ "id": null, "login": null, "display_name": null }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(
            SupportGiftSubDescriptor.matches_trigger(&TriggerConfig::new(), &as_twitch_sends_it())
        );
    }

    #[test]
    fn a_gift_sub_publishes_the_gifter_as_both_the_principal_and_the_gifter_role() {
        let stack = SupportGiftSubDescriptor.build_arg_stack(&as_twitch_sends_it());
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
    fn a_gift_sub_counts_the_gifts_the_wire_reports_and_falls_back_to_none_at_all() {
        for (wire_total, expected) in [
            (serde_json::json!(5), 5),
            (serde_json::json!(1), 1),
            (serde_json::json!(null), 0),
        ] {
            let event = gift_event(wire_total.clone(), serde_json::json!({}));
            assert_eq!(
                SupportGiftSubDescriptor
                    .build_arg_stack(&event)
                    .get("gift_count"),
                Some(&Variant::Int(expected)),
                "wire total {wire_total}"
            );
        }
    }

    #[test]
    fn a_gift_sub_leaves_the_recipient_block_empty_because_twitch_names_no_recipient() {
        let stack = SupportGiftSubDescriptor.build_arg_stack(&as_twitch_sends_it());
        for name in ["recipient_id", "recipient_name", "recipient_login"] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(String::new())),
                "'{name}'"
            );
        }
        assert_eq!(
            stack.get("recipient_platform"),
            Some(&Variant::String("twitch".to_owned()))
        );
    }

    #[test]
    fn a_named_recipient_reaches_the_recipient_block() {
        let event = gift_event(
            serde_json::json!(1),
            serde_json::json!({ "id": "444", "login": "lucky_one", "display_name": "LuckyOne" }),
        );
        let stack = SupportGiftSubDescriptor.build_arg_stack(&event);
        for (name, value) in [
            ("recipient_id", "444"),
            ("recipient_name", "LuckyOne"),
            ("recipient_login", "lucky_one"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
    }
}
