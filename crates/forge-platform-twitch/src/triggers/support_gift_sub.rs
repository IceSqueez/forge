use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

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

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let gifter_login = event
            .payload
            .get(fields::GIFTER)
            .and_then(|g| g.get(fields::GIFTER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let gifter_id = event
            .payload
            .get(fields::GIFTER)
            .and_then(|g| g.get(fields::GIFTER_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let is_anonymous = event
            .payload
            .get(fields::IS_ANONYMOUS)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let recipient_login = event
            .payload
            .get(fields::RECIPIENT)
            .and_then(|r| r.get(fields::RECIPIENT_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let recipient_id = event
            .payload
            .get(fields::RECIPIENT)
            .and_then(|r| r.get(fields::RECIPIENT_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let tier = event
            .payload
            .get(fields::TIER)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("gifter_login".to_owned(), Variant::String(gifter_login))
            .set("gifter_id".to_owned(), Variant::String(gifter_id))
            .set(
                "gifter_is_anonymous".to_owned(),
                Variant::Bool(is_anonymous),
            )
            .set(
                "recipient_login".to_owned(),
                Variant::String(recipient_login),
            )
            .set("recipient_id".to_owned(), Variant::String(recipient_id))
            .set("sub_tier".to_owned(), Variant::String(tier))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "gifter_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Gifter login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "gifter_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Gifter ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "gifter_is_anonymous".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Anonymous gifter".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "recipient_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Recipient login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "recipient_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Recipient ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "sub_tier".to_owned(),
                        kind: VariantKind::String,
                        label: "Subscription tier".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
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
