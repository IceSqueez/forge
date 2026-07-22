use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

pub(crate) struct RewardRedeemedDescriptor;

impl TriggerKindDescriptor for RewardRedeemedDescriptor {
    fn id(&self) -> &str {
        "kick.channel.reward_redeemed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Reward redeemed"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer redeems a Kick channel reward"
    }

    fn search_text(&self) -> &str {
        "kick reward redeem channel points redemption gift viewer"
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
            kind_prefix: Some("kick.channel.reward_redeemed".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let redemption_id = event
            .payload
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let reward = event.payload.get("reward");
        let reward_id = reward
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let reward_title = reward
            .and_then(|r| r.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let redeemer = event.payload.get("redeemer");
        let user_id = redeemer
            .and_then(|r| r.get("user_id"))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());
        let username = redeemer
            .and_then(|r| r.get("username"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let user_input = event
            .payload
            .get("user_input")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("redemption_id".to_owned(), Variant::String(redemption_id))
            .set("reward_id".to_owned(), Variant::String(reward_id))
            .set("reward_title".to_owned(), Variant::String(reward_title))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("username".to_owned(), Variant::String(username))
            .set("user_input".to_owned(), Variant::String(user_input))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "redemption_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Redemption ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "reward_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Reward ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "reward_title".to_owned(),
                    kind: VariantKind::String,
                    label: "Reward title".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "user_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Redeeming user ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "username".to_owned(),
                    kind: VariantKind::String,
                    label: "Redeeming username".to_owned(),
                    synthesis: Some(SynthesisHint::Username),
                },
                DeclaredVariable {
                    name: "user_input".to_owned(),
                    kind: VariantKind::String,
                    label: "User input text".to_owned(),
                    synthesis: Some(SynthesisHint::Message),
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn build_arg_stack_extracts_nested_reward_and_redeemer_fields() {
        let event = Event::new(
            EventSource::Kick,
            "kick.channel.reward_redeemed",
            serde_json::json!({
                "id": "rdm-1",
                "reward": { "id": "rwd-2", "title": "Hydrate" },
                "redeemer": { "user_id": 123, "username": "v" },
                "user_input": "text"
            }),
        );

        let stack = RewardRedeemedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("redemption_id"),
            Some(&Variant::String("rdm-1".to_owned()))
        );
        assert_eq!(
            stack.get("reward_id"),
            Some(&Variant::String("rwd-2".to_owned()))
        );
        assert_eq!(
            stack.get("reward_title"),
            Some(&Variant::String("Hydrate".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("123".to_owned()))
        );
        assert_eq!(
            stack.get("username"),
            Some(&Variant::String("v".to_owned()))
        );
        assert_eq!(
            stack.get("user_input"),
            Some(&Variant::String("text".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_leaves_redeemer_fields_empty_when_object_absent() {
        let event = Event::new(
            EventSource::Kick,
            "kick.channel.reward_redeemed",
            serde_json::json!({
                "id": "rdm-9",
                "reward": { "id": "rwd-9", "title": "Anonymous reward" },
                "user_input": ""
            }),
        );

        let stack = RewardRedeemedDescriptor.build_arg_stack(&event);

        assert_eq!(stack.get("user_id"), Some(&Variant::String(String::new())));
        assert_eq!(stack.get("username"), Some(&Variant::String(String::new())));
        assert_eq!(
            stack.get("reward_id"),
            Some(&Variant::String("rwd-9".to_owned()))
        );
    }
}
