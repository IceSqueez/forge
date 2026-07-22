use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

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
            kind_prefix: Some("channel.subscribe".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user_login = event
            .payload
            .get(fields::USER)
            .and_then(|u| u.get(fields::USER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = event
            .payload
            .get(fields::USER)
            .and_then(|u| u.get(fields::USER_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let tier = event
            .payload
            .get(fields::TIER)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let is_gift = event
            .payload
            .get(fields::IS_GIFT)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        ArgStack::new()
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("sub_tier".to_owned(), Variant::String(tier))
            .set("sub_is_gift".to_owned(), Variant::Bool(is_gift))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "user_login".to_owned(),
                        kind: VariantKind::String,
                        label: "User login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "user_id".to_owned(),
                        kind: VariantKind::String,
                        label: "User ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "sub_tier".to_owned(),
                        kind: VariantKind::String,
                        label: "Subscription tier".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "sub_is_gift".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Gifted subscription".to_owned(),
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
    fn build_arg_stack_extracts_sub_fields() {
        let stack = SupportSubscriberDescriptor.build_arg_stack(&subscribe_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("newbie".to_owned()))
        );
        assert_eq!(
            stack.get("sub_tier"),
            Some(&Variant::String("1000".to_owned()))
        );
        assert_eq!(stack.get("sub_is_gift"), Some(&Variant::Bool(false)));
    }
}
