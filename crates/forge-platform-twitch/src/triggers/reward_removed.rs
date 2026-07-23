use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::reward as fields;

pub(crate) struct RewardRemovedDescriptor;

impl TriggerKindDescriptor for RewardRemovedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel_points.reward_removed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Channel point reward removed"
    }

    fn summary(&self) -> &str {
        "Fires when a channel point reward is deleted from the broadcaster's channel"
    }

    fn search_text(&self) -> &str {
        "twitch channel points reward removed deleted"
    }

    fn icon_name(&self) -> &str {
        "star"
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
            kind_prefix: Some("twitch.channel.channel_points_custom_reward.remove".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let reward = event.payload.get(fields::REWARD);

        let reward_id = reward
            .and_then(|r| r.get(fields::REWARD_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let title = reward
            .and_then(|r| r.get(fields::REWARD_TITLE))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let cost = reward
            .and_then(|r| r.get(fields::REWARD_COST))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let prompt = reward
            .and_then(|r| r.get(fields::REWARD_PROMPT))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let is_enabled = reward
            .and_then(|r| r.get(fields::REWARD_IS_ENABLED))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        ArgStack::new()
            .set("reward.id".to_owned(), Variant::String(reward_id))
            .set("reward.title".to_owned(), Variant::String(title))
            .set("reward.cost".to_owned(), Variant::Int(cost))
            .set("reward.prompt".to_owned(), Variant::String(prompt))
            .set("reward.is_enabled".to_owned(), Variant::Bool(is_enabled))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "reward.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Reward ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "reward.title".to_owned(),
                        kind: VariantKind::String,
                        label: "Reward title".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "reward.cost".to_owned(),
                        kind: VariantKind::Int,
                        label: "Reward cost".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 0,
                            max: 1000000,
                        }),
                    },
                    DeclaredVariable {
                        name: "reward.prompt".to_owned(),
                        kind: VariantKind::String,
                        label: "Reward prompt".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "reward.is_enabled".to_owned(),
                        kind: VariantKind::Bool,
                        label: "Reward enabled".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use forge_events::Event;

    use super::*;

    fn reward_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.channel_points_custom_reward.remove",
            serde_json::json!({
                "reward": {
                    "id": "reward-7",
                    "title": "Hydrate",
                    "cost": 500,
                    "prompt": "Make the streamer drink water",
                    "is_enabled": true,
                },
            }),
        )
    }

    #[test]
    fn event_filter_targets_reward_remove_kind_from_twitch() {
        let filter = RewardRemovedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.channel_points_custom_reward.remove")
        );
    }

    #[test]
    fn build_arg_stack_marshals_cost_as_int_and_is_enabled_as_bool() {
        let stack = RewardRemovedDescriptor.build_arg_stack(&reward_event());
        assert_eq!(
            stack.get("reward.id"),
            Some(&Variant::String("reward-7".to_owned()))
        );
        assert_eq!(
            stack.get("reward.title"),
            Some(&Variant::String("Hydrate".to_owned()))
        );
        assert_eq!(stack.get("reward.cost"), Some(&Variant::Int(500)));
        assert_eq!(
            stack.get("reward.prompt"),
            Some(&Variant::String("Make the streamer drink water".to_owned()))
        );
        assert_eq!(stack.get("reward.is_enabled"), Some(&Variant::Bool(true)));
    }
}
