use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::{entity, subscription as fields};

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

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let subscriber = event.payload.get(fields::SUBSCRIBER);
        let user_id = subscriber
            .and_then(|s| s.get(entity::ID))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());

        let username = subscriber
            .and_then(|s| s.get(entity::USERNAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let months = event
            .payload
            .get(fields::MONTHS)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let tier = event
            .payload
            .get(fields::TIER)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("username".to_owned(), Variant::String(username))
            .set("months".to_owned(), Variant::Int(months))
            .set("tier".to_owned(), Variant::String(tier))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "user_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Subscriber user ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "username".to_owned(),
                    kind: VariantKind::String,
                    label: "Subscriber username".to_owned(),
                    synthesis: Some(SynthesisHint::Username),
                },
                DeclaredVariable {
                    name: "months".to_owned(),
                    kind: VariantKind::Int,
                    label: "Subscribed months".to_owned(),
                    synthesis: Some(SynthesisHint::BoundedInt { min: 1, max: 24 }),
                },
                DeclaredVariable {
                    name: "tier".to_owned(),
                    kind: VariantKind::String,
                    label: "Subscription tier".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sub_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.channel.subscribed",
            serde_json::json!({
                "subscriber": { "id": 123, "username": "new_subscriber" },
                "months": 3,
                "tier": "tier1"
            }),
        )
    }

    #[test]
    fn build_arg_stack_extracts_sub_fields() {
        let stack = SubDescriptor.build_arg_stack(&sub_event());
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("123".to_owned()))
        );
        assert_eq!(
            stack.get("username"),
            Some(&Variant::String("new_subscriber".to_owned()))
        );
        assert_eq!(stack.get("months"), Some(&Variant::Int(3)));
        assert_eq!(
            stack.get("tier"),
            Some(&Variant::String("tier1".to_owned()))
        );
    }
}
