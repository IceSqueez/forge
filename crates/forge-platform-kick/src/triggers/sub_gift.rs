use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

pub(crate) struct SubGiftDescriptor;

impl TriggerKindDescriptor for SubGiftDescriptor {
    fn id(&self) -> &str {
        "kick.channel.subscription_gift"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "Gifted subscriptions"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer gifts subscriptions to the Kick channel"
    }

    fn search_text(&self) -> &str {
        "kick gift sub gifted subscriptions community"
    }

    fn icon_name(&self) -> &str {
        "gift"
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
            kind_prefix: Some("kick.channel.subscription_gift".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let gifter_id = event
            .payload
            .get("gifter_user_id")
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());

        let gifter_username = event
            .payload
            .get("gifter_username")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let count = event
            .payload
            .get("gifted_usernames")
            .and_then(|v| v.as_array())
            .map_or(0usize, |a| a.len())
            .to_string();

        let tier = event
            .payload
            .get("subscription")
            .and_then(|s| s.get("slug"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("gifter_id".to_owned(), Variant::String(gifter_id))
            .set(
                "gifter_username".to_owned(),
                Variant::String(gifter_username),
            )
            .set("count".to_owned(), Variant::String(count))
            .set("tier".to_owned(), Variant::String(tier))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "gifter_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Gifter user ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "gifter_username".to_owned(),
                    kind: VariantKind::String,
                    label: "Gifter username".to_owned(),
                    synthesis: Some(SynthesisHint::Username),
                },
                DeclaredVariable {
                    name: "count".to_owned(),
                    kind: VariantKind::String,
                    label: "Gifted subscription count".to_owned(),
                    synthesis: None,
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

    fn gift_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.channel.subscription_gift",
            serde_json::json!({
                "gifter_user_id": 200,
                "gifter_username": "generous_viewer",
                "gifted_usernames": ["user_a", "user_b", "user_c"],
                "subscription": { "slug": "tier1" }
            }),
        )
    }

    #[test]
    fn build_arg_stack_extracts_gift_fields() {
        let stack = SubGiftDescriptor.build_arg_stack(&gift_event());
        assert_eq!(
            stack.get("gifter_id"),
            Some(&Variant::String("200".to_owned()))
        );
        assert_eq!(
            stack.get("gifter_username"),
            Some(&Variant::String("generous_viewer".to_owned()))
        );
        assert_eq!(stack.get("count"), Some(&Variant::String("3".to_owned())));
        assert_eq!(
            stack.get("tier"),
            Some(&Variant::String("tier1".to_owned()))
        );
    }
}
